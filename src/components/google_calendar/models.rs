/// Simplified calendar event representation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct CalendarEvent {
    pub id: String,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub created: Option<String>,
    pub start_date_time: Option<String>,
    pub start_date: Option<String>,
    pub end_date_time: Option<String>,
    pub end_date: Option<String>,
}
