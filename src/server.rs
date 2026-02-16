use std::{
    collections::HashMap,
    mem,
    sync::{Arc, Condvar, Mutex},
    task::{
        Poll::{Pending, Ready},
        Waker,
    },
    thread::{self, JoinHandle},
};

const EVENT_BUFFER_SIZE: usize = 1024;

use tokio::task::JoinSet;

use crate::{
    connection::Connection,
    epoll::{EPoll, EPollEvent},
    error_utils::MaybeFatal,
    event::EventFD,
    response::{MaybeResponse, Response},
    router::BaseRouter,
    socket::{Socket, SocketAcceptError, SocketListeningError},
};

struct ResolvedResponse {
    response: Response,
    connection_descriptor: usize,
}

enum ServerEvent {
    EPollEvents(Vec<EPollEvent>),
    ResponseReady(ResolvedResponse),
}

pub struct HTTPServer {
    socket: Socket,
    connections: HashMap<usize, Connection>,
    router: BaseRouter,
    epoll: Arc<EPoll>,
    pending_events: JoinSet<ServerEvent>,
}

#[derive(Debug)]
pub enum HTTPServerRunError {
    SocketListeningError(SocketListeningError),
    SocketAcceptError(SocketAcceptError),
}

impl MaybeFatal for HTTPServerRunError {
    fn is_fatal(&self) -> bool {
        match self {
            Self::SocketListeningError(socket_listening_error) => matches!(
                socket_listening_error,
                SocketListeningError::ListeningFailed(_)
            ),
            Self::SocketAcceptError(socket_accept_error) => socket_accept_error.is_fatal(),
        }
    }
}

impl HTTPServer {
    pub fn new(socket: Socket, router: BaseRouter) -> Self {
        // let event_buffer = Arc::new(Mutex::new(Vec::with_capacity(EVENT_BUFFER_SIZE)));
        let socket_file_descriptor = socket.get_file_descriptor();
        let epoll = Arc::new(EPoll::new().expect("Failed to create epoll to wake server."));
        epoll.add(socket_file_descriptor, true, false).expect("");
        Self {
            socket,
            connections: HashMap::new(),
            router,
            epoll,
            pending_events: JoinSet::new(),
        }
    }

    async fn handle_event(&mut self, event: EPollEvent) -> Result<(), HTTPServerRunError> {
        let file_descriptor = event.file_descriptor();
        if file_descriptor == self.socket.get_file_descriptor() {
            if let Err(err) = self.accept_connections()
                && err.is_fatal()
            {
                return Err(err);
            }
            return Ok(());
        }
        let mut entry = match self.connections.entry(file_descriptor) {
            std::collections::hash_map::Entry::Occupied(occupied_entry) => occupied_entry,
            std::collections::hash_map::Entry::Vacant(_vacant_entry) => {
                let _ = self.epoll.delete(file_descriptor);
                return Ok(());
            }
        };
        match (event, entry.get_mut()) {
            (event, connection) if event.readable() && connection.is_reading() => {
                match connection.read() {
                    Ok(mut request) => {
                        println!("Received request:\n{}", request);
                        assert!(connection.is_awaiting_response());
                        match self.router.route(connection, &mut request) {
                            MaybeResponse::Now(response) => {
                                connection
                                    .begin_response(&response)
                                    .expect("Connection not ready to write after checking.");
                                assert!(connection.is_writing());
                                assert!(!connection.is_reading());
                                self.epoll
                                    .modify(file_descriptor, false, true)
                                    .expect("Failed to update epoll");
                            }
                            MaybeResponse::Later(future) => {
                                self.pending_events.spawn(async move {
                                    ServerEvent::ResponseReady(ResolvedResponse {
                                        response: future.await.expect("Request handler failed."),
                                        connection_descriptor: file_descriptor,
                                    })
                                });
                            }
                        };
                    }
                    Err(read_error) => {
                        println!("Error while reading: {:?}", read_error);
                        if read_error.is_fatal() {
                            self.epoll
                                .delete(file_descriptor)
                                .expect("Failed to update epoll");
                            entry.remove();
                        }
                    }
                };
                Ok(())
            }
            (event, connection) if event.writable() && connection.is_writing() => {
                if let Err(error) = connection.write() {
                    println!("Error while writing: {:?}", error);
                }
                if !connection.is_alive() {
                    self.epoll
                        .delete(file_descriptor)
                        .expect("Failed to update epoll");
                    entry.remove();
                }
                Ok(())
            }
            (event, connection)
                if event.readable()
                    && (connection.is_writing() || connection.is_awaiting_response()) =>
            {
                // Stale event
                assert!(!connection.is_reading());
                Ok(())
            }
            (event, connection) => {
                panic!(
                    "Unexpected event / connection pair: event: {:?} r: {} w: {}\nconnection: {:?}",
                    event,
                    event.readable(),
                    event.writable(),
                    connection
                );
            }
        }
    }

    pub async fn run(&mut self) -> HTTPServerRunError {
        if !self.socket.is_listening()
            && let Err(err) = self.socket.start_listening()
        {
            return HTTPServerRunError::SocketListeningError(err);
        }
        let notifier =
            Box::leak(Box::new(HTTPServerNotifier::new(self.epoll.clone()))) as &HTTPServerNotifier;
        self.pending_events.spawn(notifier);
        loop {
            match self.pending_events.join_next().await.expect(
                "Server event tasks are empty, but at least the epoll task should be present.",
            ) {
                Ok(ServerEvent::EPollEvents(events)) => {
                    for event in events {
                        match self.handle_event(event).await {
                            Ok(_) => {} // Nothing went wrong, event was handled successfully
                            Err(error) => return error,
                        }
                    }
                    self.pending_events.spawn(notifier);
                }
                Ok(ServerEvent::ResponseReady(resolved_response)) => {
                    if let Some(connection) = self
                        .connections
                        .get_mut(&resolved_response.connection_descriptor)
                    {
                        connection
                            .begin_response(&resolved_response.response)
                            .expect("Connection not ready to write after checking.");
                        assert!(connection.is_writing());
                        assert!(!connection.is_reading());
                        self.epoll
                            .modify(resolved_response.connection_descriptor, false, true)
                            .expect("Failed to update epoll");
                    }
                }
                Err(_) => panic!("Server task panicked."),
            }
        }
    }

    pub fn accept_connections(&mut self) -> Result<(), HTTPServerRunError> {
        match self.socket.accept_connection() {
            Ok(descriptor) => {
                assert!(
                    self.connections
                        .insert(descriptor, Connection::new(descriptor))
                        .is_none()
                );
                self.epoll
                    .add(descriptor, true, false)
                    .expect("Failed to monitor new connection.");
                println!("Established new connection.");
                Ok(())
            }
            Err(err) => Err(HTTPServerRunError::SocketAcceptError(err)),
        }
    }
}

#[derive(Debug)]
enum WakerStatus {
    New(Waker),
    Waiting,
    Closed,
}

impl const Default for WakerStatus {
    fn default() -> Self {
        Self::Waiting
    }
}

impl WakerStatus {
    const fn take(&mut self) -> Self {
        mem::take(self)
    }
}

#[derive(Debug, Clone)]
struct NotiferSharedObjects {
    notifier: Arc<Condvar>,
    buffer: Arc<Mutex<Vec<EPollEvent>>>,
    waker: Arc<Mutex<WakerStatus>>,
    check_guard: Arc<Mutex<()>>,
}

impl Default for NotiferSharedObjects {
    fn default() -> Self {
        Self {
            notifier: Default::default(),
            buffer: Arc::new(Mutex::new(Vec::with_capacity(EVENT_BUFFER_SIZE))),
            waker: Default::default(),
            check_guard: Default::default(),
        }
    }
}

struct HTTPServerNotifierSleeper {
    shared_data: NotiferSharedObjects,
    poller: Arc<EPoll>,
}

impl HTTPServerNotifierSleeper {
    fn run(&self) {
        let mut exit_signal: bool = false;
        let mut waker: Option<Waker> = None;
        while !exit_signal {
            while let buffer = &mut self
                .shared_data
                .buffer
                .lock()
                .expect("Server thread has panicked!")
                && buffer.is_empty()
            {
                self.poller.wait(buffer);
            }
            let _data_ref = self
                .shared_data
                .notifier
                .wait_while(
                    self.shared_data
                        .check_guard
                        .lock()
                        .expect("Server thread has panicked!"),
                    |_| {
                        let buffer = self
                            .shared_data
                            .buffer
                            .lock()
                            .expect("Server thread has panicked!");
                        let buffer_is_nonempty = !buffer.is_empty();
                        drop(buffer);
                        let mut waker_status = self
                            .shared_data
                            .waker
                            .lock()
                            .expect("Server thread has panicked!");
                        match waker_status.take() {
                            WakerStatus::New(new_waker) => {
                                waker.replace(new_waker);
                            }
                            WakerStatus::Closed => exit_signal = true,
                            WakerStatus::Waiting => {}
                        };
                        drop(waker_status);
                        if buffer_is_nonempty && let Some(waker) = waker.take() {
                            waker.wake()
                        }
                        buffer_is_nonempty && !exit_signal
                    },
                )
                .expect("Server thread has panicked!");
        }
    }
}

struct HTTPServerNotifier {
    notifier_thread: JoinHandle<()>,
    shared_data: NotiferSharedObjects,
    epoll_waker: EventFD,
}

impl HTTPServerNotifier {
    fn new(poller: Arc<EPoll>) -> Self {
        let shared_data = NotiferSharedObjects::default();
        let data_clone = shared_data.clone();
        let epoll_waker = EventFD::new().expect("Failed to create event.");
        poller
            .add(epoll_waker.get_file_descriptor(), true, false)
            .expect("Failed to register event.");
        let notifier_thread = thread::spawn(|| {
            let sleeper = HTTPServerNotifierSleeper {
                shared_data: data_clone,
                poller,
            };
            sleeper.run();
        });

        Self {
            notifier_thread,
            shared_data,
            epoll_waker,
        }
    }
}

impl Drop for HTTPServerNotifier {
    fn drop(&mut self) {
        self.epoll_waker.set();
        let mut attempts = 0;
        while !self.notifier_thread.is_finished() && attempts < 100000 {
            match self.shared_data.waker.lock() {
                Ok(mut waker) => {
                    *waker = WakerStatus::Closed;
                }
                Err(_) => return,
            }
            self.shared_data.notifier.notify_all();
            attempts += 1;
        }
    }
}

impl Future for &HTTPServerNotifier {
    type Output = ServerEvent;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let guard = self
            .shared_data
            .check_guard
            .lock()
            .expect("Notifier thread panicked.");
        let mut waker = self
            .shared_data
            .waker
            .lock()
            .expect("Notifier thread panicked.");
        if let Ok(mut buffer) = self.shared_data.buffer.try_lock()
            && !buffer.is_empty()
        {
            let events = Vec::from_iter(buffer.drain(..));
            *waker = WakerStatus::Waiting;
            drop(waker);
            drop(buffer);
            drop(guard);
            self.shared_data.notifier.notify_one();
            return Ready(ServerEvent::EPollEvents(events));
        }

        *waker = WakerStatus::New(cx.waker().clone());
        drop(waker);
        drop(guard);
        self.shared_data.notifier.notify_one();
        Pending
    }
}
