use super::errors::ResourceUsageError;
use super::value_objects::*;

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceUsage {
    id: UsageId,
    user: User,
    time_period: TimePeriod,
    resources: Vec<Resource>,
    notes: Option<String>,
}

impl ResourceUsage {
    pub fn new(
        id: UsageId,
        user: User,
        time_period: TimePeriod,
        resources: Vec<Resource>,
        notes: Option<String>,
    ) -> Result<Self, ResourceUsageError> {
        if resources.is_empty() {
            return Err(ResourceUsageError::NoResourceItems);
        }

        Ok(Self {
            id,
            user,
            time_period,
            resources,
            notes,
        })
    }

    pub fn id(&self) -> &UsageId {
        &self.id
    }

    pub fn user(&self) -> &User {
        &self.user
    }

    pub fn time_period(&self) -> &TimePeriod {
        &self.time_period
    }

    pub fn resources(&self) -> &Vec<Resource> {
        &self.resources
    }

    pub fn notes(&self) -> Option<&String> {
        self.notes.as_ref()
    }

    pub fn update_time_period(&mut self, new_time_period: TimePeriod) {
        self.time_period = new_time_period;
    }

    pub fn update_notes(&mut self, notes: String) {
        self.notes = Some(notes);
    }
}
