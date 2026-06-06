//! Database migration logic.
//!
//! Migrations are handled inline in HostDb::open() for simplicity.
//! This module exists for future multi-step migrations.

// Currently no standalone migrations — all handled in db::HostDb::migrate()
