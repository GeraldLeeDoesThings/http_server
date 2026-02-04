#![warn(clippy::all, clippy::nursery)]
#![feature(const_default)]
#![feature(const_trait_impl)]
#![feature(lock_value_accessors)]
#![feature(map_try_insert)]

pub mod connection;
pub mod epoll;
pub mod error_utils;
pub mod event;
pub mod handler;
pub mod header;
pub mod protocol;
pub mod request;
pub mod response;
pub mod router;
pub mod server;
pub mod socket;
