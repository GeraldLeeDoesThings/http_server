#![warn(clippy::all, clippy::nursery)]
#![feature(map_try_insert)]

pub mod connection;
pub mod error_utils;
pub mod handler;
pub mod header;
pub mod protocol;
pub mod request;
pub mod response;
pub mod router;
pub mod server;
pub mod socket;
