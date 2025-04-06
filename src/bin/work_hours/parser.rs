use crate::model::{WorkDay, WorkDayExtraction, WorkSchedule};
use base64::{self, engine::Engine};
use chrono::{Datelike, Duration, Local, NaiveDate};
use rig::completion::message::{Image, ImageMediaType};
use rig::completion::{Chat, Message};
use rig::message::ContentFormat;
use rig::providers::gemini::Client as GeminiClient;
use serde_json::from_str;
use std::env;
use tracing::{error, info, warn};

/// Prompt for the Gemini agent to extract the work schedule from the image
const PROMPT: &str = "Analyze the provided image, which is a work schedule.

First, identify the following reference details from the **current image** (for context only, do not include in the final output):
1.  The **target employee name**: {{NAME}} (You must specify the employee name here each time you use this prompt).
2.  The **full date range** shown in the schedule's title (e.g., \"{{DATE_RANGE}}\" or similar).
3.  The **year** indicated by the date range in the title (e.g., {{YEAR}}).
4.  The **start date** ({{START_FINNISH}}) and **end date** ({{END_FINNISH}}) from the identified range.
5.  All the individual **day/month column headers** visible within that identified date range (e.g., \"{{DAY1}}\", \"{{DAY2}}\", ... through \"{{SUNDAY1}}\" and up to \"{{SUNDAY2}}\").

Your primary task is to extract the complete work schedule specifically for the employee **{{NAME}}**, covering every single day from the **start date** ({{START_DATE}}) to the **end date** ({{END_DATE}}) identified from the image title (typically spanning {{DAYS_COUNT}} days as shown in the grid, covering week numbers {{WEEK_NUMBERS}}).

Output the schedule as a single JSON array. Each object in the array represents one day and MUST follow this exact structure:
{
  \"date\": \"YYYY-MM-DD\",
  \"work_hours\": \"VALUE_FROM_CELL\"
}

Adhere strictly to these rules for generating the JSON output:

1.  **Date Generation:**
    *   Create a JSON object for **every single calendar date** from the **identified start date** to the **identified end date** derived *directly from the image title*. The total number of objects must match the number of days in that identified range.
    *   Format the 'date' value as \"YYYY-MM-DD\", using the **year identified from the image title**. Correctly map the day and month from each **column header found in the image grid** for the relevant period.

2.  **Work Hours Extraction ('work_hours' value):**
    *   For each date, locate the cell at the intersection of the **{{NAME}}'s row** and the **correct date column** in the grid. Accurate row/column mapping is critical.
    *   The 'work_hours' value MUST be the **primary content** written in that specific cell.
    *   **Handling Cell Content:**
        *   **Time Ranges:** If the cell contains a time range (e.g., \"12-20.30\", \"7.30-15.30\", \"8.35-16\", \"9-17L\"), use that exact string. Normalize any commas in times to periods (e.g., \"7,30-15.30\" becomes \"7.30-15.30\").
        *   **Codes:** If the cell contains only a code (e.g., \"x\", \"v\", \"vv\", \"VL\", \"S\", \"tst\"), use that exact code string.
        *   **Text / Combined Entries:** If the cell contains text (e.g., \"Toive\") or combined text/codes (e.g., \"Toive\" written above \"vp\"), use the combined representation (\"Toive vp\"). If text spans multiple lines (like \"Pai-\" above \"kalla\"), combine them (\"Pai-kalla\").
        *   **Ignoring Secondary Numbers:** Many cells contain a primary entry (time/code/text) *above* a separate numerical value (like '8', '7,5', '8.5', '8,25'). **You MUST use ONLY the primary entry as the 'work_hours' value.** For example, if a cell shows \"12-20.30\" on top and \"8,5\" below it, the correct 'work_hours' value is \"12-20.30\". The number below must be ignored in these cases.
        *   **Handling Empty Cells:** If the specific cell for the target employee on a given date is **visually blank or contains no primary text/code/time**, the 'work_hours' value MUST be an empty string `\"\"`. Ensure this rule is applied correctly even for days at the very beginning or end of the identified date range.

3.  **Accuracy and Completeness:**
    *   Ensure the output covers the entire period from the **identified start date** ({{START_DATE}}) to the **identified end date** ({{END_DATE}}) without gaps.
    *   Double-check that the content extracted for each date precisely matches the primary content in the **{{NAME}}'s row** for that specific date column in the image grid. Avoid row or column misalignments.

4. **CRITICAL:** THERE MIGHT BE EMPTY DAYS IN THE SCHEDULE. YOU MUST INCLUDE THEM IN THE OUTPUT. THERE MIGHT BE MULTIPLE EMPTY DAYS IN A ROW. YOU MUST INCLUDE ALL OF THEM.


Provide **only** the final JSON array as the output. Do not include any introductory text, explanations, list of identified details, markdown formatting (`json`), or comments before or after the JSON data.";

pub async fn parse_schedule_image(
    employee_name: &str,
    image_data: &[u8],
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Result<WorkSchedule, String> {
    // Log the parsing action
    info!("Parsing schedule image for employee: {}", employee_name);
    info!("Image size: {} bytes", image_data.len());

    if let (Some(start), Some(end)) = (start_date, end_date) {
        info!("Date range: {} to {}", start, end);
    }

    #[cfg(feature = "web-interface")]
    {
        // Preprocess the image to align the grid properly
        info!("Preprocessing image to align grid...");
        let processed_image_data =
            match crate::image_processing::preprocess_schedule_image(image_data) {
                Ok(data) => {
                    info!(
                        "Image preprocessing successful. New size: {} bytes",
                        data.len()
                    );
                    data
                }
                Err(e) => {
                    warn!("Image preprocessing failed: {}. Using original image.", e);
                    image_data.to_vec()
                }
            };

        // Use env to get API key
        let api_key = env::var("GEMINI_API_KEY")
            .map_err(|_| "GEMINI_API_KEY environment variable not set".to_string())?;

        // Get model name from environment variable
        let model =
            env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-pro-exp-03-25".to_string());

        info!("Using Gemini model: {}", model);

        // Initialize Gemini client with API key
        let gemini_client = GeminiClient::new(&api_key);

        // Calculate default dates if not provided
        let (start_date_str, end_date_str) = match (start_date, end_date) {
            (Some(start), Some(end)) => (start.to_string(), end.to_string()),
            _ => {
                // Get tomorrow's date
                let tomorrow = Local::now().date_naive() + Duration::days(1);

                // Find the next Monday from tomorrow
                let days_until_monday = (8 - tomorrow.weekday().num_days_from_monday()) % 7;
                let start = tomorrow + Duration::days(days_until_monday as i64);

                // Calculate end date (3 weeks from start date)
                let end = start + Duration::days(21 - 1);

                (
                    start.format("%Y-%m-%d").to_string(),
                    end.format("%Y-%m-%d").to_string(),
                )
            }
        };

        // Parse the start and end dates
        let start_date_obj = NaiveDate::parse_from_str(&start_date_str, "%Y-%m-%d")
            .map_err(|_| "Invalid start date format".to_string())?;
        let end_date_obj = NaiveDate::parse_from_str(&end_date_str, "%Y-%m-%d")
            .map_err(|_| "Invalid end date format".to_string())?;

        // Calculate week numbers
        let start_week = start_date_obj.iso_week().week();
        let end_week = end_date_obj.iso_week().week();

        // Create week numbers string (e.g., "10, 11, 12" or "10-12")
        let week_numbers = if start_week == end_week {
            format!("{}", start_week)
        } else if start_week + 1 == end_week {
            format!("{}, {}", start_week, end_week)
        } else {
            format!("{}-{}", start_week, end_week)
        };

        // Generate week examples (e.g., "vko 10, vko 11, vko 12")
        let mut week_examples = String::new();
        for week in start_week..=end_week {
            if !week_examples.is_empty() {
                week_examples.push_str(", ");
            }
            week_examples.push_str(&format!("vko {}", week));
        }

        // Format the start date in Finnish format (e.g., "24.3.")
        let start_finnish = format!("{}.{}.", start_date_obj.day(), start_date_obj.month());
        // Format the end date in Finnish format (e.g., "13.4.")
        let end_finnish = format!("{}.{}.", end_date_obj.day(), end_date_obj.month());
        // Get year from the start date
        let year = start_date_obj.year();

        // Calculate the number of days in the range (inclusive)
        let days_count = (end_date_obj - start_date_obj).num_days() + 1;

        // Format date range in Finnish format for title (e.g., "24.3.-13.4.2025")
        let date_range = format!("{}-{}{}", start_finnish, end_finnish, year);

        // Format first day (e.g., "Ma 24.3.")
        let day1 = format!("Ma {}.{}.", start_date_obj.day(), start_date_obj.month());

        // Format second day (e.g., "Ti 25.3.")
        let day2_date = start_date_obj + Duration::days(1);
        let day2 = format!("Ti {}.{}.", day2_date.day(), day2_date.month());

        // Format Sunday in the first week (for example dates)
        let first_sunday_offset = (7 - start_date_obj.weekday().num_days_from_monday()) % 7;
        let first_sunday = start_date_obj + Duration::days(first_sunday_offset as i64);
        let sunday1 = format!("Su {}.{}.", first_sunday.day(), first_sunday.month());

        // Format Sunday in the last week
        let sunday2 = format!("Su {}.{}.", end_date_obj.day(), end_date_obj.month());

        // Replace placeholders in prompt with actual values
        let prompt = PROMPT
            .replace("{{NAME}}", employee_name)
            .replace("{{DATE_RANGE}}", &date_range)
            .replace("{{YEAR}}", &year.to_string())
            .replace("{{START_FINNISH}}", &start_finnish)
            .replace("{{END_FINNISH}}", &end_finnish)
            .replace("{{DAY1}}", &day1)
            .replace("{{DAY2}}", &day2)
            .replace("{{SUNDAY1}}", &sunday1)
            .replace("{{SUNDAY2}}", &sunday2)
            .replace("{{START_DATE}}", &start_date_str)
            .replace("{{END_DATE}}", &end_date_str)
            .replace("{{WEEK_NUMBERS}}", &week_numbers)
            .replace(
                "{{FIRST_SUNDAY_DATE}}",
                &first_sunday.format("%Y-%m-%d").to_string(),
            )
            .replace("{{DAYS_COUNT}}", &days_count.to_string());

        // Base64 encode the image
        let base64_image = base64::engine::general_purpose::STANDARD.encode(processed_image_data);

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
            .preamble("You are an expert at parsing work schedules from images and get paid hundreds of thousands of dollars for your work so you are very good at it, but if you fail, you will be fired and lose your job and your family will starve. You are also very good at Finnish.")
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
