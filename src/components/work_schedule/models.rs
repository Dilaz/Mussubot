/// Represents a work schedule entry for an employee
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkScheduleEntry {
    pub date: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub is_day_off: bool,
    pub notes: Option<String>,
}

impl WorkScheduleEntry {
    /// Create a new work schedule entry
    pub fn new(date: String) -> Self {
        Self {
            date,
            start_time: None,
            end_time: None,
            is_day_off: false,
            notes: None,
        }
    }

    /// Format the schedule as a human-readable string
    pub fn format(&self) -> String {
        if self.is_day_off {
            return "Day off".to_string();
        }

        match (self.start_time.as_ref(), self.end_time.as_ref()) {
            (Some(start), Some(end)) => format!("{} - {}", start, end),
            (Some(start), None) => format!("Starting at {}", start),
            (None, Some(end)) => format!("Ending at {}", end),
            (None, None) => "No scheduled hours".to_string(),
        }
    }
}

/// Represents a collection of work schedule entries for an employee
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct EmployeeSchedule {
    pub employee: String,
    pub schedule: Vec<WorkScheduleEntry>,
} 