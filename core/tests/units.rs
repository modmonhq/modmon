//! Unit tests of single modules that need only the public API. They live
//! here, not next to the code, so that the crate's own test build stays
//! small: `scripts/test-build-memory.sh` prints how much memory that build
//! needs. One binary holds them all, because every test binary links the
//! whole library.

mod temp_dirs;

#[path = "units/build_manager.rs"]
mod build_manager;
#[path = "units/dashboard_query.rs"]
mod dashboard_query;
#[path = "units/drained_restart.rs"]
mod drained_restart;
#[path = "units/environment_redaction.rs"]
mod environment_redaction;
#[path = "units/job_scheduler.rs"]
mod job_scheduler;
#[path = "units/local_ca.rs"]
mod local_ca;
