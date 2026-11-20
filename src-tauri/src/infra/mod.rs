//! Disk, network and archive access: everything that touches the outside world, under one name.

pub mod archive;
pub mod binaries;
pub mod clock;
pub mod credentials;
pub mod deployment;
pub mod disk;
pub mod file_cache;
pub mod fs;
pub mod http;
pub mod io;
pub mod json;
pub mod log;
pub mod memo;
pub mod profiles_file;
pub mod sidecar;
pub mod storage;
pub mod store;
pub mod tasks;
pub mod text;
