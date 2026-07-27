//! Scrat application layer: use-cases orchestrating the domain via its
//! repository ports. Depends only on `scrat-domain` — never on a concrete
//! storage or presentation technology.

pub mod account_service;
pub mod category_service;
pub mod transaction_service;
