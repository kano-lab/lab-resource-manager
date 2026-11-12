use super::policy::{AuthorizationError, AuthorizationPolicy};
use crate::domain::aggregates::reservation::{Reservation, ReservedSlot};
use crate::domain::common::EmailAddress;

/// 予約と予約枠の認可ポリシー
///
/// 予約者本人だけが、予約とその予約枠を変更・取り消しできる。
#[derive(Debug, Clone, Default)]
pub struct ReservationAuthorizationPolicy;

impl ReservationAuthorizationPolicy {
    pub fn new() -> Self {
        Self
    }

    fn require_holder(
        actor: &EmailAddress,
        holder: &EmailAddress,
        action: &str,
        target: String,
    ) -> Result<(), AuthorizationError> {
        if holder == actor {
            return Ok(());
        }
        Err(AuthorizationError::Forbidden {
            actor: actor.clone(),
            action: action.to_string(),
            resource: target,
        })
    }
}

impl AuthorizationPolicy<ReservedSlot> for ReservationAuthorizationPolicy {
    fn authorize_update(
        &self,
        actor: &EmailAddress,
        slot: &ReservedSlot,
    ) -> Result<(), AuthorizationError> {
        Self::require_holder(
            actor,
            slot.holder_email(),
            "update",
            format!("ReservedSlot({})", slot.id().as_str()),
        )
    }

    fn authorize_delete(
        &self,
        actor: &EmailAddress,
        slot: &ReservedSlot,
    ) -> Result<(), AuthorizationError> {
        Self::require_holder(
            actor,
            slot.holder_email(),
            "cancel",
            format!("ReservedSlot({})", slot.id().as_str()),
        )
    }
}

impl AuthorizationPolicy<Reservation> for ReservationAuthorizationPolicy {
    fn authorize_update(
        &self,
        actor: &EmailAddress,
        reservation: &Reservation,
    ) -> Result<(), AuthorizationError> {
        Self::require_holder(
            actor,
            reservation.holder_email(),
            "update",
            format!("Reservation({})", reservation.id().as_str()),
        )
    }

    fn authorize_delete(
        &self,
        actor: &EmailAddress,
        reservation: &Reservation,
    ) -> Result<(), AuthorizationError> {
        Self::require_holder(
            actor,
            reservation.holder_email(),
            "cancel",
            format!("Reservation({})", reservation.id().as_str()),
        )
    }
}
