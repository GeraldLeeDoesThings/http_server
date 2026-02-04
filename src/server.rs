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

use crate::{
    connection::Connection,
    epoll::{EPoll, EPollEvent},
    error_utils::MaybeFatal,
    event::EventFD,
    router::BaseRouter,
    socket::{Socket, SocketAcceptError, SocketListeningError},
};

pub struct HTTPServer {
    socket: Socket,
    connections: HashMap<usize, Connection>,
    router: BaseRouter,
    epoll: Arc<EPoll>,
    notifier: HTTPServerNotifier,
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
        let event_buffer = Arc::new(Mutex::new(Vec::with_capacity(EVENT_BUFFER_SIZE)));
        let socket_file_descriptor = socket.get_file_descriptor();
        let epoll = Arc::new(EPoll::new().expect("Failed to create epoll to wake server."));
        epoll.add(socket_file_descriptor, true, false).expect("");
        Self {
            socket,
            connections: HashMap::new(),
            router,
            epoll: epoll.clone(),
            notifier: HTTPServerNotifier::new(event_buffer, epoll),
        }
    }

    pub async fn run(&mut self) -> HTTPServerRunError {
        if !self.socket.is_listening()
            && let Err(err) = self.socket.start_listening()
        {
            return HTTPServerRunError::SocketListeningError(err);
        }
        loop {
            for event in (&mut self.notifier).await {
                let file_descriptor = event.file_descriptor();
                if file_descriptor == self.socket.get_file_descriptor() {
                    if let Err(err) = self.accept_connections()
                        && err.is_fatal()
                    {
                        return err;
                    }
                    continue;
                }
                let mut entry = match self.connections.entry(file_descriptor) {
                    std::collections::hash_map::Entry::Occupied(occupied_entry) => occupied_entry,
                    std::collections::hash_map::Entry::Vacant(_vacant_entry) => {
                        self.epoll
                            .delete(file_descriptor)
                            .expect("Failed to update epoll");
                        continue;
                    }
                };
                match (event, entry.get_mut()) {
                    (event, connection) if event.readable() && connection.is_reading() => {
                        let read_result = connection.read();
                        if let Ok(mut request) = read_result {
                            println!("Received request:\n{}", request);
                            assert!(connection.is_awaiting_response());
                            let response = self.router.route(connection, &mut request);
                            connection
                                .begin_response(&response)
                                .expect("Connection not ready to write after checking.");
                            assert!(connection.is_writing());
                            assert!(!connection.is_reading());
                            self.epoll
                                .modify(file_descriptor, false, true)
                                .expect("Failed to update epoll");
                        } else {
                            println!("Error while reading: {:?}", read_result);
                            self.epoll
                                .delete(file_descriptor)
                                .expect("Failed to update epoll");
                            entry.remove();
                        }
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
                    }
                    _ => unreachable!("epoll state mismatch."),
                }
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
    Success,
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

#[derive(Debug, Default, Clone)]
struct NotiferSharedData {
    start_notifier: Arc<Condvar>,
    start_mutex: Arc<Mutex<bool>>,
    waker_notifier: Arc<Condvar>,
    waker_mutex: Arc<Mutex<WakerStatus>>,
}

struct HTTPServerNotifierSleeper {
    shared_data: NotiferSharedData,
    buffer: Arc<Mutex<Vec<EPollEvent>>>,
    poller: Arc<EPoll>,
}

impl HTTPServerNotifierSleeper {
    fn run(&self) {
        loop {
            *self
                .shared_data
                .start_notifier
                .wait_while(
                    self.shared_data
                        .start_mutex
                        .lock()
                        .expect("Server thread has panicked!"),
                    |&mut start_signal| !start_signal,
                )
                .expect("Server thread has panicked!") = false;
            self.poller
                .wait(&mut self.buffer.lock().expect("Server thread has panicked!"));
            loop {
                let waker_status = self
                    .shared_data
                    .waker_notifier
                    .wait_while(
                        self.shared_data
                            .waker_mutex
                            .lock()
                            .expect("Server thread has panicked!"),
                        |waker_status| matches!(waker_status, WakerStatus::Waiting),
                    )
                    .expect("Server thread has panicked!")
                    .take();
                match waker_status {
                    WakerStatus::New(waker) => waker.wake(),
                    WakerStatus::Success => break,
                    WakerStatus::Waiting => unreachable!("Guarded by condition variable."),
                    WakerStatus::Closed => return,
                }
            }
        }
    }
}

struct HTTPServerNotifier {
    notifier_thread: JoinHandle<()>,
    buffer: Arc<Mutex<Vec<EPollEvent>>>,
    shared_data: NotiferSharedData,
    epoll_waker: EventFD,
}

impl HTTPServerNotifier {
    fn new(buffer: Arc<Mutex<Vec<EPollEvent>>>, poller: Arc<EPoll>) -> Self {
        let shared_data = NotiferSharedData::default();
        let buffer_clone = buffer.clone();
        let data_clone = shared_data.clone();
        let epoll_waker = EventFD::new().expect("Failed to create event.");
        poller
            .add(epoll_waker.get_file_descriptor(), true, false)
            .expect("Failed to register event.");
        let notifier_thread = thread::spawn(|| {
            let sleeper = HTTPServerNotifierSleeper {
                shared_data: data_clone,
                buffer: buffer_clone,
                poller,
            };
            sleeper.run();
        });

        Self {
            notifier_thread,
            buffer,
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
            if self
                .shared_data
                .waker_mutex
                .set(WakerStatus::Closed)
                .is_err()
                || self.shared_data.start_mutex.set(true).is_err()
            {
                return;
            }
            self.shared_data.start_notifier.notify_all();
            self.shared_data.waker_notifier.notify_all();
            attempts += 1;
        }
    }
}

impl Future for &mut HTTPServerNotifier {
    type Output = Vec<EPollEvent>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut buffer = self.buffer.lock().expect("Notifier thread panicked.");
        if !buffer.is_empty() {
            let events = Vec::from_iter(buffer.drain(..));
            drop(buffer);
            *self
                .shared_data
                .waker_mutex
                .lock()
                .expect("Notifier thread panicked.") = WakerStatus::Success;
            self.shared_data.waker_notifier.notify_one();
            return Ready(events);
        }

        *self
            .shared_data
            .waker_mutex
            .lock()
            .expect("Notifier thread panicked.") = WakerStatus::New(cx.waker().clone());
        self.shared_data.waker_notifier.notify_one();
        *self
            .shared_data
            .start_mutex
            .lock()
            .expect("Notifier thread panicked.") = true;
        self.shared_data.start_notifier.notify_one();
        Pending
    }
}
