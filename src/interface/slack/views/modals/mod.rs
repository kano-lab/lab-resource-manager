//! Modal view builders

mod registration;
mod reservation;

pub use registration::create_register_email_modal;
pub use reservation::create_reserve_modal;
