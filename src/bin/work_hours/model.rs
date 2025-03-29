use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a day's work hours
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkDay {
    /// The date of the workday (YYYY-MM-DD)
    pub date: String,
    /// Start time if working that day (HH:MM)
    pub start_time: Option<String>,
    /// End time if working that day (HH:MM)
    pub end_time: Option<String>,
    /// Whether this is a day off
    pub is_day_off: bool,
    /// Any notes for this day
    pub notes: Option<String>,
}

/// Represents a complete work schedule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkSchedule {
    /// The employee name
    pub employee_name: String,
    /// The days in the schedule
    pub days: Vec<WorkDay>,
    /// When the schedule was last updated
    pub last_updated: DateTime<Utc>,
}

impl WorkSchedule {
    /// Create a new work schedule for an employee
    pub fn new(employee_name: String) -> Self {
        Self {
            employee_name,
            days: Vec::new(),
            last_updated: Utc::now(),
        }
    }

    /// Add a day to the schedule
    pub fn add_day(&mut self, day: WorkDay) {
        self.days.push(day);
        self.last_updated = Utc::now();
    }
}

/// Response from the AI parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleParsingResult {
    /// Was the parsing successful
    pub success: bool,
    /// Any error message if parsing failed
    pub error: Option<String>,
    /// The extracted work days
    pub days: Vec<WorkDay>,
}

/// Database trait for storing and retrieving work schedules
#[async_trait::async_trait]
pub trait WorkHoursDb: Send + Sync + 'static {
    /// Get a schedule for an employee
    async fn get_schedule(&self, employee_name: &str) -> Result<Option<WorkSchedule>, String>;

    /// Store a schedule for an employee
    async fn set_schedule(
        &self,
        employee_name: &str,
        schedule: &WorkSchedule,
    ) -> Result<(), String>;

    /// List all employee names with schedules
    async fn list_employees(&self) -> Result<Vec<String>, String>;

    /// Delete a schedule for an employee
    async fn delete_schedule(&self, employee_name: &str) -> Result<(), String>;
}

/// In-memory implementation of the database (for testing)
#[derive(Debug, Default)]
pub struct InMemoryDb {
    schedules: tokio::sync::RwLock<HashMap<String, WorkSchedule>>,
}

#[async_trait::async_trait]
impl WorkHoursDb for InMemoryDb {
    async fn get_schedule(&self, employee_name: &str) -> Result<Option<WorkSchedule>, String> {
        let schedules = self.schedules.read().await;
        Ok(schedules.get(employee_name).cloned())
    }

    async fn set_schedule(
        &self,
        employee_name: &str,
        schedule: &WorkSchedule,
    ) -> Result<(), String> {
        let mut schedules = self.schedules.write().await;
        schedules.insert(employee_name.to_string(), schedule.clone());
        Ok(())
    }

    async fn list_employees(&self) -> Result<Vec<String>, String> {
        let schedules = self.schedules.read().await;
        Ok(schedules.keys().cloned().collect())
    }

    async fn delete_schedule(&self, employee_name: &str) -> Result<(), String> {
        let mut schedules = self.schedules.write().await;
        schedules.remove(employee_name);
        Ok(())
    }
}

// Define the target extraction structure to match the expected JSON format
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub(crate) struct WorkDayExtraction {
    pub date: String,
    pub work_hours: String,
}
