//! The daemon as one node of a cluster target, the steps of a rollout on
//! it, and the access sync run as root on a host. (Moved here from the
//! daemon's unit tests, unchanged in what they check: they need only the
//! public API, and the unit-test build is at its memory limit.)

mod cluster;
mod fixture;
mod rollout;
mod root_sync;
#[path = "../temp_dirs/mod.rs"]
mod temp_dirs;
