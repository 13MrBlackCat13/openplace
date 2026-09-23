// sqlx tuple rows and builder-style handlers trip a few style lints that
// would only add noise here; everything else is clippy-clean.
#![allow(clippy::type_complexity)]
#![allow(clippy::too_many_arguments)]

pub mod auth;
pub mod config;
pub mod discord;
pub mod error;
pub mod extract;
pub mod migrate;
pub mod rate_limiter;
pub mod routes;
pub mod services;
pub mod setup;
pub mod state;
pub mod utils;
