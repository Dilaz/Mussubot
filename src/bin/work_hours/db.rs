use crate::model::{WorkHoursDb, WorkSchedule};
use async_trait::async_trait;
use redis::{AsyncCommands, Client as RedisClient};
use std::env;
use tracing::info;

/// Redis keys - matching those used in the main application
mod keys {
    pub const WORK_HOURS_EMPLOYEES: &str = "work_hours:employees";
    pub const WORK_HOURS_DAY_PREFIX: &str = "work_hours:day:";
    pub const WORK_HOURS_DATES_PREFIX: &str = "work_hours:dates:";
    pub const WORK_HOURS_SCHEDULE_PREFIX: &str = "work_hours:schedule:";
    /// 30 days in seconds
    pub const EXPIRY_SECONDS: i64 = 30 * 24 * 60 * 60;
}

/// Direct Redis database implementation
pub struct RedisDB {
    client: RedisClient,
}

impl RedisDB {
    /// Create a new Redis database connection
    pub fn new() -> Result<Self, String> {
        // Use the REDIS_URL environment variable or default to localhost
        let redis_url =
            env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

        info!("Connecting to Redis at {}", redis_url);

        let client = RedisClient::open(redis_url)
            .map_err(|e| format!("Failed to create Redis client: {}", e))?;

        Ok(Self { client })
    }

    /// Get a Redis connection from the client
    async fn get_connection(&self) -> Result<redis::aio::MultiplexedConnection, String> {
        self.client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("Failed to connect to Redis: {}", e))
    }
}

#[async_trait]
impl WorkHoursDb for RedisDB {
    async fn get_schedule(&self, employee_name: &str) -> Result<Option<WorkSchedule>, String> {
        let key = format!("{}{}", keys::WORK_HOURS_SCHEDULE_PREFIX, employee_name);

        // Get a connection
        let mut conn = self.get_connection().await?;

        // Check if the key exists
        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| format!("Redis EXISTS error: {}", e))?;

        if !exists {
            return Ok(None);
        }

        // Get the data
        let data: String = conn
            .get(&key)
            .await
            .map_err(|e| format!("Redis GET error: {}", e))?;

        // Parse the JSON
        let schedule: WorkSchedule =
            serde_json::from_str(&data).map_err(|e| format!("JSON parse error: {}", e))?;

        Ok(Some(schedule))
    }

    async fn set_schedule(
        &self,
        employee_name: &str,
        schedule: &WorkSchedule,
    ) -> Result<(), String> {
        // Get a connection
        let mut conn = self.get_connection().await?;

        // Serialize the schedule
        let json = serde_json::to_string(schedule)
            .map_err(|e| format!("JSON serialization error: {}", e))?;

        // Store the main schedule
        let key = format!("{}{}", keys::WORK_HOURS_SCHEDULE_PREFIX, employee_name);

        conn.set::<_, _, ()>(&key, &json)
            .await
            .map_err(|e| format!("Redis SET error: {}", e))?;

        // Set expiry for the schedule key
        conn.expire::<_, ()>(&key, keys::EXPIRY_SECONDS)
            .await
            .map_err(|e| format!("Redis EXPIRE error: {}", e))?;

        // Add to the set of employees
        conn.sadd::<_, _, ()>(keys::WORK_HOURS_EMPLOYEES, employee_name)
            .await
            .map_err(|e| format!("Redis SADD error: {}", e))?;

        // Store individual days for quick access
        let dates_key = format!("{}{}", keys::WORK_HOURS_DATES_PREFIX, employee_name);

        for day in &schedule.days {
            // Add to the set of dates
            conn.sadd::<_, _, ()>(&dates_key, &day.date)
                .await
                .map_err(|e| format!("Redis SADD error: {}", e))?;

            // Store the individual day data
            let day_key = format!(
                "{}{}:{}",
                keys::WORK_HOURS_DAY_PREFIX,
                employee_name,
                day.date
            );
            let day_json = serde_json::to_string(day)
                .map_err(|e| format!("JSON day serialization error: {}", e))?;

            conn.set::<_, _, ()>(&day_key, &day_json)
                .await
                .map_err(|e| format!("Redis SET error: {}", e))?;

            // Set expiry for each day key
            conn.expire::<_, ()>(&day_key, keys::EXPIRY_SECONDS)
                .await
                .map_err(|e| format!("Redis EXPIRE error: {}", e))?;
        }

        // Set expiry for the dates key
        conn.expire::<_, ()>(&dates_key, keys::EXPIRY_SECONDS)
            .await
            .map_err(|e| format!("Redis EXPIRE error: {}", e))?;

        info!(
            "Stored schedule for {} with {} days",
            employee_name,
            schedule.days.len()
        );
        Ok(())
    }

    async fn list_employees(&self) -> Result<Vec<String>, String> {
        // Get a connection
        let mut conn = self.get_connection().await?;

        // Get all employees from the set
        let employees: Vec<String> = conn
            .smembers(keys::WORK_HOURS_EMPLOYEES)
            .await
            .map_err(|e| format!("Redis SMEMBERS error: {}", e))?;

        Ok(employees)
    }

    async fn delete_schedule(&self, employee_name: &str) -> Result<(), String> {
        // Get a connection
        let mut conn = self.get_connection().await?;

        // Get all dates for this employee
        let dates_key = format!("{}{}", keys::WORK_HOURS_DATES_PREFIX, employee_name);

        let dates: Vec<String> = conn
            .smembers(&dates_key)
            .await
            .map_err(|e| format!("Redis SMEMBERS error: {}", e))?;

        // Delete each day's data
        for date in &dates {
            let day_key = format!("{}{}:{}", keys::WORK_HOURS_DAY_PREFIX, employee_name, date);

            conn.del::<_, ()>(&day_key)
                .await
                .map_err(|e| format!("Redis DEL error: {}", e))?;
        }

        // Delete the dates set
        conn.del::<_, ()>(&dates_key)
            .await
            .map_err(|e| format!("Redis DEL error: {}", e))?;

        // Delete the main schedule
        let schedule_key = format!("{}{}", keys::WORK_HOURS_SCHEDULE_PREFIX, employee_name);

        conn.del::<_, ()>(&schedule_key)
            .await
            .map_err(|e| format!("Redis DEL error: {}", e))?;

        // Remove from the employees set
        conn.srem::<_, _, ()>(keys::WORK_HOURS_EMPLOYEES, employee_name)
            .await
            .map_err(|e| format!("Redis SREM error: {}", e))?;

        info!("Deleted schedule for {}", employee_name);
        Ok(())
    }
}
