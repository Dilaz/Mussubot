use super::actor::{WorkScheduleActor, WorkScheduleActorHandle};
use super::models::{EmployeeSchedule, WorkScheduleEntry};
use crate::components::redis_service::RedisActorHandle;
use crate::config::Config;
use crate::error::BotResult;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

/// Handle for interacting with the Work Schedule actor
#[derive(Clone)]
pub struct WorkScheduleHandle {
    actor_handle: WorkScheduleActorHandle,
    _actor_task: Arc<JoinHandle<()>>,
}

impl WorkScheduleHandle {
    /// Create a new WorkScheduleHandle and spawn the actor
    pub fn new(config: Arc<RwLock<Config>>, redis_handle: RedisActorHandle) -> Self {
        // Create the actor and get its handle
        let (mut actor, handle) = WorkScheduleActor::new(config, redis_handle);

        // Spawn a task to run the actor
        let actor_task = tokio::spawn(async move {
            actor.run().await;
        });

        Self {
            actor_handle: handle,
            _actor_task: Arc::new(actor_task),
        }
    }

    /// Get all employees with schedules
    pub async fn get_employees(&self) -> BotResult<Vec<String>> {
        self.actor_handle.get_employees().await
    }

    /// Get schedule for a specific employee
    pub async fn get_schedule_for_employee(
        &self,
        employee: impl Into<String>,
    ) -> BotResult<EmployeeSchedule> {
        self.actor_handle.get_schedule_for_employee(employee).await
    }

    /// Get schedule for all employees on a specific date
    pub async fn get_schedule_for_date(
        &self,
        date: impl Into<String>,
    ) -> BotResult<HashMap<String, WorkScheduleEntry>> {
        self.actor_handle.get_schedule_for_date(date).await
    }

    /// Get schedule for an employee in a date range
    pub async fn get_schedule_for_date_range(
        &self,
        employee: impl Into<String>,
        start_date: impl Into<String>,
        end_date: impl Into<String>,
    ) -> BotResult<EmployeeSchedule> {
        self.actor_handle
            .get_schedule_for_date_range(employee, start_date, end_date)
            .await
    }

    /// Get a single schedule entry for employee and date
    pub async fn get_entry_for_employee_date(
        &self,
        employee: impl Into<String>,
        date: impl Into<String>,
    ) -> BotResult<WorkScheduleEntry> {
        let employee = employee.into();
        let date = date.into();

        // Get all dates for this employee to check if the date exists
        let schedule = self.get_schedule_for_employee(employee.clone()).await?;

        // Find the entry for the given date
        for entry in schedule.schedule {
            if entry.date == date {
                return Ok(entry);
            }
        }

        // If no entry is found, create a default one
        Ok(WorkScheduleEntry::new(date))
    }

    /// Shutdown the actor
    pub async fn shutdown(&self) -> BotResult<()> {
        self.actor_handle.shutdown().await
    }
}
