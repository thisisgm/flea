//! Backend job regression suites share the backend helpers through this test-only module.
use super::*;

#[path = "jobs_fix1_tests.rs"]
mod jobs_fix1_tests;

#[path = "jobs_fix2_tests.rs"]
mod jobs_fix2_tests;

#[path = "jobs_tests.rs"]
mod jobs_tests;

#[path = "jobs_regression_tests.rs"]
mod jobs_regression_tests;

#[path = "jobs_extract_eta_tests.rs"]
mod jobs_extract_eta_tests;
