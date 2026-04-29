//! Integration test entry point
//!
//! This file allows running integration tests with:
//! ```bash
//! cargo test --test integration
//! ```

#[path = "integration/e2e_test.rs"]
mod e2e_test;

#[path = "integration/mongodb_container_test.rs"]
mod mongodb_container_test;

#[path = "integration/postgres_container_test.rs"]
mod postgres_container_test;

#[path = "integration/postgres_test.rs"]
mod postgres_test;

#[path = "integration/uploader_test.rs"]
mod uploader_test;
