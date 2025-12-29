//! Integration tests for Datapace Agent
//!
//! These tests require external resources (databases, network) and are
//! run separately from unit tests.
//!
//! # Running Integration Tests
//!
//! ```bash
//! # Run with Docker databases
//! cargo test --test integration
//!
//! # Run specific database tests
//! cargo test --test integration postgres
//! cargo test --test integration mysql
//! ```

mod postgres_test;
// mod mysql_test;  // Enable when MySQL support is added
