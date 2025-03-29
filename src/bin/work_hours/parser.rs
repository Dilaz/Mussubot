use crate::model::{WorkDay, WorkDayExtraction, WorkSchedule};
use base64::{self, engine::Engine};
use chrono::NaiveDate;
use rig::completion::message::{Image, ImageMediaType};
use rig::completion::{Chat, Message};
use rig::message::ContentFormat;
use rig::providers::gemini::Client as GeminiClient;
use serde_json::from_str;
use std::env;
use tracing::{error, info, warn};

/// Prompt for the Gemini agent to extract the work schedule from the image.
const PROMPT: &str = "Analyze the provided image, which is a work schedule titled \"Työvuorot ajalle **DATE_RANGE**\".

Your task is to extract the complete work schedule for the employee named **{NAME}** covering the entire period shown (**DATE_RANGE**).

Output the schedule as a JSON array where each object represents a single day. Each object must have the following exact structure:
{
  \"date\": \"YYYY-MM-DD\",
  \"work_hours\": \"VALUE_FROM_CELL\"
}

Follow these SUPER strict rules:
1.  **Date Format:** The 'date' value must be in \"YYYY-MM-DD\" format. Use the year 2025 as indicated in the schedule title. Correctly map the day and month from the column headers (e.g., \"24.3.\" becomes \"2025-03-24\", \"1.4.\" becomes \"2025-04-01\", \"13.4.\" becomes \"2025-04-13\").
2.  **Work Hours Value:** The 'work_hours' value must be the *exact string* found in the corresponding cell for that employee and date.
    *   If the cell contains a time range (e.g., \"7-15\", \"12-20.15\", \"8.35-16\"), use that exact string.
    *   If the cell contains a code (e.g., \"v\", \"x\", \"vp\", \"Toive\", \"Toive vp\", \"Paikalla\", \"Palkat\", \"Tst\", \"Koulutus\", \"lo\", \"mat\", \"vv\", \"VL\", \"S\"), use that exact code string as found.
    *   If a cell contains a time with a comma like \"7,30-16\", **normalize the comma to a period** and use that string: \"7.30-16\". Use periods consistently for decimal points in times (e.g., \"20.15\").
    *   If the cell for a specific date in the employee's row is **completely empty** (contains no text or code), use an empty string `\"\"` for the 'work_hours' value. Be extremely careful identifying truly empty cells, especially if there are multiple consecutive ones at the start or end of the row segment for a week.
3.  **Completeness:** Ensure you include one JSON object for *every single date* from 2025-03-24 to 2025-04-13 inclusive for the specified employee. There should be exactly 21 objects in the final array.
4.  **Accuracy:** Meticulously check the employee's specific row and the corresponding date column for each entry. Cross-reference the date column headers (Ma, Ti, Ke, To, Pe, La, Su) and the dates (24.3. ... 13.4.) across the weeks (vko 13, vko 14, vko 15). Accuracy in matching the cell content to the date is paramount.

Provide **only** the JSON array as the output. Do not include any introductory text, explanations, markdown formatting (`json`), or comments before or after the JSON data.";

pub async fn parse_schedule_image(
    employee_name: &str,
    image_data: &[u8],
) -> Result<WorkSchedule, String> {
    // Log the parsing action
    info!("Parsing schedule image for employee: {}", employee_name);
    info!("Image size: {} bytes", image_data.len());

    #[cfg(feature = "web-interface")]
    {
        // Use env to get API key
        let api_key = env::var("GEMINI_API_KEY")
            .map_err(|_| "GEMINI_API_KEY environment variable not set".to_string())?;

        // Get model name from environment variable
        let model =
            env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-pro-exp-03-25".to_string());

        info!("Using Gemini model: {}", model);

        // Initialize Gemini client with API key
        let gemini_client = GeminiClient::new(&api_key);

        // Replace {NAME} placeholder in prompt with actual employee name
        let prompt = PROMPT.replace("{NAME}", employee_name);

        // Base64 encode the image
        let base64_image = base64::engine::general_purpose::STANDARD.encode(image_data);

        let image = Image {
            data: base64_image,
            media_type: Some(ImageMediaType::JPEG),
            format: Some(ContentFormat::Base64),
            detail: None,
        };

        // Create an image message by direct conversion from Image struct
        let image_message = Message::from(image);

        // Create combined message vector
        let messages = vec![image_message];

        // Get the text response from Gemini
        let agent = gemini_client
            .agent(&model)
            .preamble("You are a helpful assistant that parses work schedules from images.")
            .temperature(0.0)
            .build();

        let response = agent
            .chat(prompt, messages)
            .await
            .map_err(|err| format!("Failed to complete request: {}", err))?;

        info!("Received response: {}", response);

        // Extract JSON array from the response
        let extracted_days: Vec<WorkDayExtraction> = extract_json_array(&response)
            .map_err(|e| format!("Failed to extract valid JSON from response: {}", e))?;

        // Convert to our internal WorkSchedule format
        let mut schedule = WorkSchedule::new(employee_name.to_string());

        for day in extracted_days {
            // Parse date
            if let Ok(_date) = NaiveDate::parse_from_str(&day.date, "%Y-%m-%d") {
                if day.work_hours.is_empty() {
                    // Empty work hours cell
                    schedule.add_day(WorkDay {
                        date: day.date,
                        start_time: None,
                        end_time: None,
                        is_day_off: false,
                        notes: None,
                    });
                } else if day.work_hours.contains('-') {
                    // Parse time range like "7-15"
                    let parts: Vec<&str> = day.work_hours.split('-').collect();
                    if parts.len() == 2 {
                        let start = normalize_time(parts[0]);
                        let end = normalize_time(parts[1]);

                        schedule.add_day(WorkDay {
                            date: day.date,
                            start_time: Some(start),
                            end_time: Some(end),
                            is_day_off: false,
                            notes: None,
                        });
                    } else {
                        // Can't parse time range, store as note
                        schedule.add_day(WorkDay {
                            date: day.date,
                            start_time: None,
                            end_time: None,
                            is_day_off: false,
                            notes: Some(day.work_hours),
                        });
                    }
                } else {
                    // Special codes like "x", "v", etc. or other text
                    let is_day_off = day.work_hours.to_lowercase() == "x"
                        || day.work_hours.to_lowercase() == "v";

                    schedule.add_day(WorkDay {
                        date: day.date,
                        start_time: None,
                        end_time: None,
                        is_day_off,
                        notes: Some(day.work_hours),
                    });
                }
            } else {
                warn!("Invalid date format: {}", day.date);
            }
        }

        Ok(schedule)
    }

    #[cfg(not(feature = "web-interface"))]
    {
        // Mock implementation for when the feature is not enabled
        mock_parse_schedule(employee_name)
    }
}

// New helper function to extract a valid JSON array from potentially messy text
#[cfg(feature = "web-interface")]
fn extract_json_array(text: &str) -> Result<Vec<WorkDayExtraction>, String> {
    // Try to explicitly handle ```json ... ``` format first
    let trimmed_text = text.trim();

    // Handle ```json ... ``` format
    let trimmed_text = if trimmed_text.starts_with("```json") && trimmed_text.ends_with("```") {
        trimmed_text
            .trim_start_matches("```json")
            .trim_end_matches("```")
            .trim()
    }
    // Handle ``` ... ``` format (without language specification)
    else if trimmed_text.starts_with("```") && trimmed_text.ends_with("```") {
        trimmed_text
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed_text
    };

    info!("Trimmed text: {}", trimmed_text);

    // Try to parse the trimmed text
    if let Ok(days) = from_str::<Vec<WorkDayExtraction>>(trimmed_text) {
        return Ok(days);
    }

    // Fall back to our previous methods
    // Try to parse the response as JSON directly first
    if let Ok(days) = from_str::<Vec<WorkDayExtraction>>(text) {
        return Ok(days);
    }

    // Look for the first '[' and last ']' to extract the JSON array
    if let (Some(start), Some(end)) = (text.find('['), text.rfind(']')) {
        if start < end {
            let json_part = &text[start..=end];
            match from_str(json_part) {
                Ok(days) => return Ok(days),
                Err(e) => {
                    error!("Found JSON-like structure but couldn't parse it: {}", e);
                    error!("Extracted JSON-like content: {}", json_part);
                }
            }
        }
    }

    // If we get here, try to clean up the text and look for code blocks
    let clean_text = text.replace("```json", "").replace("```", "");

    // Try again with cleaned text
    if let Ok(days) = from_str::<Vec<WorkDayExtraction>>(&clean_text) {
        return Ok(days);
    }

    // Look for array in cleaned text
    if let (Some(start), Some(end)) = (clean_text.find('['), clean_text.rfind(']')) {
        if start < end {
            let json_part = &clean_text[start..=end];
            match from_str(json_part) {
                Ok(days) => return Ok(days),
                Err(e) => {
                    error!(
                        "Found cleaned JSON-like structure but couldn't parse it: {}",
                        e
                    );
                    error!("Cleaned JSON-like content: {}", json_part);
                }
            }
        }
    }

    // Last resort - try to construct valid JSON manually from the parts
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    if lines
        .iter()
        .any(|l| l.contains("\"date\"") && l.contains("\"work_hours\""))
    {
        // This looks like it contains JSON objects - try to reconstruct the array
        let array_text = format!("[{}]", lines.join(""));
        if let Ok(days) = from_str::<Vec<WorkDayExtraction>>(&array_text) {
            return Ok(days);
        }
    }

    Err("Could not extract valid JSON from the model response".to_string())
}

#[cfg(not(feature = "web-interface"))]
fn mock_parse_schedule(employee_name: &str) -> Result<WorkSchedule, String> {
    let mut schedule = WorkSchedule::new(employee_name.to_string());

    // Get current date
    let today = Local::now().date_naive();

    // Find the Monday of this week
    let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);

    // Create a week's worth of workdays
    for i in 0..7 {
        let day = monday + Duration::days(i);
        let day_str = day.format("%Y-%m-%d").to_string();

        // Weekdays (Monday-Friday)
        if i < 5 {
            schedule.add_day(WorkDay {
                date: day_str,
                start_time: Some("08:00".to_string()),
                end_time: Some("16:00".to_string()),
                is_day_off: false,
                notes: None,
            });
        } else {
            // Weekend (Saturday-Sunday)
            schedule.add_day(WorkDay {
                date: day_str,
                start_time: None,
                end_time: None,
                is_day_off: true,
                notes: Some("Weekend".to_string()),
            });
        }
    }

    Ok(schedule)
}

#[cfg(feature = "web-interface")]
// Helper function to normalize time strings
// Converts formats like "7" to "07:00"
fn normalize_time(time_str: &str) -> String {
    let time_str = time_str.trim();

    // If it already has a colon, assume HH:MM format
    if time_str.contains(':') {
        return time_str.to_string();
    }

    // Otherwise assume it's just hours
    match time_str.parse::<u32>() {
        Ok(hours) if hours < 24 => format!("{:02}:00", hours),
        _ => time_str.to_string(), // Return as is if invalid
    }
}
