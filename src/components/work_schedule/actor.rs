use crate::components::redis_service::RedisActorHandle;
use crate::components::work_schedule::models::{EmployeeSchedule, WorkScheduleEntry};
use crate::config::Config;
use crate::error::{work_schedule_error, BotResult};
use chrono::NaiveDate;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info};

// Redis key constants
pub mod keys {
    pub const WORK_HOURS_EMPLOYEES: &str = "work_hours:employees";
    pub const WORK_HOURS_DAY_PREFIX: &str = "work_hours:day:";
    pub const WORK_HOURS_DATES_PREFIX: &str = "work_hours:dates:";
}

/// The Work Schedule actor that processes messages
pub struct WorkScheduleActor {
    _config: Arc<RwLock<Config>>,
    redis_handle: RedisActorHandle,
    command_rx: mpsc::Receiver<WorkScheduleCommand>,
}

/// Commands that can be sent to the Work Schedule actor
pub enum WorkScheduleCommand {
    GetEmployees(mpsc::Sender<BotResult<Vec<String>>>),
    GetScheduleForEmployee(String, mpsc::Sender<BotResult<EmployeeSchedule>>),
    GetScheduleForDate(String, mpsc::Sender<BotResult<HashMap<String, WorkScheduleEntry>>>),
    GetScheduleForDateRange(
        String,
        String,
        String,
        mpsc::Sender<BotResult<EmployeeSchedule>>,
    ),
    Shutdown,
}

/// Handle for communicating with the Work Schedule actor
#[derive(Clone)]
pub struct WorkScheduleActorHandle {
    command_tx: mpsc::Sender<WorkScheduleCommand>,
}

impl WorkScheduleActorHandle {
    /// Get a list of all employees with schedules
    pub async fn get_employees(&self) -> BotResult<Vec<String>> {
        let (response_tx, mut response_rx) = mpsc::channel(1);
        self.command_tx
            .send(WorkScheduleCommand::GetEmployees(response_tx))
            .await
            .map_err(|e| work_schedule_error(&format!("Actor mailbox error: {}", e)))?;

        response_rx
            .recv()
            .await
            .ok_or_else(|| work_schedule_error("Response channel closed"))?
    }

    /// Get schedule for a specific employee
    pub async fn get_schedule_for_employee(
        &self,
        employee: impl Into<String>,
    ) -> BotResult<EmployeeSchedule> {
        let (response_tx, mut response_rx) = mpsc::channel(1);
        self.command_tx
            .send(WorkScheduleCommand::GetScheduleForEmployee(
                employee.into(),
                response_tx,
            ))
            .await
            .map_err(|e| work_schedule_error(&format!("Actor mailbox error: {}", e)))?;

        response_rx
            .recv()
            .await
            .ok_or_else(|| work_schedule_error("Response channel closed"))?
    }

    /// Get schedule for all employees on a specific date
    pub async fn get_schedule_for_date(
        &self,
        date: impl Into<String>,
    ) -> BotResult<HashMap<String, WorkScheduleEntry>> {
        let (response_tx, mut response_rx) = mpsc::channel(1);
        self.command_tx
            .send(WorkScheduleCommand::GetScheduleForDate(
                date.into(),
                response_tx,
            ))
            .await
            .map_err(|e| work_schedule_error(&format!("Actor mailbox error: {}", e)))?;

        response_rx
            .recv()
            .await
            .ok_or_else(|| work_schedule_error("Response channel closed"))?
    }

    /// Get schedule for an employee in a date range
    pub async fn get_schedule_for_date_range(
        &self,
        employee: impl Into<String>,
        start_date: impl Into<String>,
        end_date: impl Into<String>,
    ) -> BotResult<EmployeeSchedule> {
        let (response_tx, mut response_rx) = mpsc::channel(1);
        self.command_tx
            .send(WorkScheduleCommand::GetScheduleForDateRange(
                employee.into(),
                start_date.into(),
                end_date.into(),
                response_tx,
            ))
            .await
            .map_err(|e| work_schedule_error(&format!("Actor mailbox error: {}", e)))?;

        response_rx
            .recv()
            .await
            .ok_or_else(|| work_schedule_error("Response channel closed"))?
    }

    /// Shutdown the actor
    pub async fn shutdown(&self) -> BotResult<()> {
        let _ = self.command_tx.send(WorkScheduleCommand::Shutdown).await;
        Ok(())
    }
}

impl WorkScheduleActor {
    /// Create a new actor and return its handle
    pub fn new(
        config: Arc<RwLock<Config>>,
        redis_handle: RedisActorHandle,
    ) -> (Self, WorkScheduleActorHandle) {
        let (command_tx, command_rx) = mpsc::channel(32);

        let actor = Self {
            _config: config,
            redis_handle,
            command_rx,
        };

        let handle = WorkScheduleActorHandle { command_tx };

        (actor, handle)
    }

    /// Start the actor's processing loop
    pub async fn run(&mut self) {
        info!("Work Schedule actor started");

        // Process commands
        while let Some(cmd) = self.command_rx.recv().await {
            match cmd {
                WorkScheduleCommand::GetEmployees(response_tx) => {
                    let result = self.get_employees_from_redis().await;
                    let _ = response_tx.send(result).await;
                }
                WorkScheduleCommand::GetScheduleForEmployee(employee, response_tx) => {
                    let result = self.get_schedule_for_employee(&employee).await;
                    let _ = response_tx.send(result).await;
                }
                WorkScheduleCommand::GetScheduleForDate(date, response_tx) => {
                    let result = self.get_schedule_for_date(&date).await;
                    let _ = response_tx.send(result).await;
                }
                WorkScheduleCommand::GetScheduleForDateRange(
                    employee,
                    start_date,
                    end_date,
                    response_tx,
                ) => {
                    let result = self
                        .get_schedule_for_date_range(&employee, &start_date, &end_date)
                        .await;
                    let _ = response_tx.send(result).await;
                }
                WorkScheduleCommand::Shutdown => {
                    info!("Work Schedule actor shutting down");
                    break;
                }
            }
        }

        info!("Work Schedule actor shut down");
    }

    /// Get all employees from Redis
    async fn get_employees_from_redis(&self) -> BotResult<Vec<String>> {
        let mut custom_cmd = redis::cmd("SMEMBERS");
        custom_cmd.arg(keys::WORK_HOURS_EMPLOYEES);

        let result: BotResult<Vec<String>> = self
            .redis_handle
            .run_command(custom_cmd)
            .await
            .map_err(|e| work_schedule_error(&format!("Failed to get employees: {}", e)));

        result
    }

    /// Get schedule for a specific employee
    async fn get_schedule_for_employee(&self, employee: &str) -> BotResult<EmployeeSchedule> {
        // First get all dates for this employee
        let dates_key = format!("{}{}", keys::WORK_HOURS_DATES_PREFIX, employee);
        
        let mut custom_cmd = redis::cmd("SMEMBERS");
        custom_cmd.arg(dates_key);

        let _dates: Vec<String> = self
            .redis_handle
            .run_command(custom_cmd)
            .await
            .map_err(|e| work_schedule_error(&format!("Failed to get dates: {}", e)))?;

        let _schedule = EmployeeSchedule {
            employee: employee.to_string(),
            schedule: Vec::new(),
        };

        // Calculate the date range for this week (Monday to Sunday)
        let now = chrono::Local::now();
        let today = now.date_naive();
        let weekday_num = today.format("%u").to_string().parse::<u32>().unwrap_or(1); // 1=Monday, 7=Sunday
        let days_since_monday = weekday_num - 1;
        let monday = today.checked_sub_signed(chrono::Duration::days(days_since_monday as i64)).unwrap_or(today);
        let sunday = monday.checked_add_signed(chrono::Duration::days(6)).unwrap_or(monday);

        let start_date = monday.format("%Y-%m-%d").to_string();
        let end_date = sunday.format("%Y-%m-%d").to_string();

        // Use the date range method to get the current week's schedule
        return self.get_schedule_for_date_range(employee, &start_date, &end_date).await;
    }

    /// Get a single schedule entry for employee and date
    async fn get_entry_for_employee_date(
        &self,
        employee: &str,
        date: &str,
    ) -> BotResult<WorkScheduleEntry> {
        let key = format!("{}{}{}{}", keys::WORK_HOURS_DAY_PREFIX, employee, ":", date);
        
        let mut custom_cmd = redis::cmd("GET");
        custom_cmd.arg(key);

        let entry_json: Option<String> = self
            .redis_handle
            .run_command(custom_cmd)
            .await
            .map_err(|e| {
                work_schedule_error(&format!(
                    "Failed to get entry for {} on {}: {}",
                    employee, date, e
                ))
            })?;

        if let Some(json) = entry_json {
            let entry: WorkScheduleEntry = serde_json::from_str(&json).map_err(|e| {
                work_schedule_error(&format!(
                    "Failed to deserialize entry for {} on {}: {}",
                    employee, date, e
                ))
            })?;
            Ok(entry)
        } else {
            // If no entry is found, create a default one
            Ok(WorkScheduleEntry::new(date.to_string()))
        }
    }

    /// Get schedule for all employees on a specific date
    async fn get_schedule_for_date(
        &self,
        date: &str,
    ) -> BotResult<HashMap<String, WorkScheduleEntry>> {
        let employees = self.get_employees_from_redis().await?;
        let mut result = HashMap::new();

        for employee in employees {
            match self.get_entry_for_employee_date(&employee, date).await {
                Ok(entry) => {
                    result.insert(employee, entry);
                }
                Err(e) => {
                    error!(
                        "Failed to get entry for {} on {}: {}",
                        employee, date, e
                    );
                    // Continue with the next employee
                }
            }
        }

        Ok(result)
    }

    /// Get schedule for an employee in a date range
    async fn get_schedule_for_date_range(
        &self,
        employee: &str,
        start_date: &str,
        end_date: &str,
    ) -> BotResult<EmployeeSchedule> {
        // Parse the dates
        let start = NaiveDate::parse_from_str(start_date, "%Y-%m-%d").map_err(|e| {
            work_schedule_error(&format!("Failed to parse start date {}: {}", start_date, e))
        })?;

        let end = NaiveDate::parse_from_str(end_date, "%Y-%m-%d").map_err(|e| {
            work_schedule_error(&format!("Failed to parse end date {}: {}", end_date, e))
        })?;

        // Get all dates for this employee
        let dates_key = format!("{}{}", keys::WORK_HOURS_DATES_PREFIX, employee);
        
        let mut custom_cmd = redis::cmd("SMEMBERS");
        custom_cmd.arg(dates_key);

        let all_dates: HashSet<String> = self
            .redis_handle
            .run_command::<Vec<String>>(custom_cmd)
            .await
            .map_err(|e| work_schedule_error(&format!("Failed to get dates: {}", e)))?
            .into_iter()
            .collect();

        let mut schedule = EmployeeSchedule {
            employee: employee.to_string(),
            schedule: Vec::new(),
        };

        // For each date in the range, get the schedule entry if it exists
        let mut current = start;
        while current <= end {
            let date_str = current.format("%Y-%m-%d").to_string();
            
            // If the date exists in Redis, get the entry
            if all_dates.contains(&date_str) {
                match self.get_entry_for_employee_date(employee, &date_str).await {
                    Ok(entry) => {
                        schedule.schedule.push(entry);
                    }
                    Err(e) => {
                        error!(
                            "Failed to get entry for {} on {}: {}",
                            employee, date_str, e
                        );
                    }
                }
            } else {
                // Otherwise create a default entry
                schedule.schedule.push(WorkScheduleEntry::new(date_str));
            }
            
            // Move to the next day
            current = current.succ_opt().unwrap_or(end);
        }

        Ok(schedule)
    }
} 