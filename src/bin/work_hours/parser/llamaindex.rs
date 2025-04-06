use crate::model::{WorkDay, WorkDayExtraction, WorkSchedule};
use chrono::{Datelike, Local, NaiveDate};
use reqwest::{header, multipart, Client};
use serde::Deserialize;
use serde_json::{from_str, Value};
use std::env;
use tracing::{debug, info, warn};

#[cfg(feature = "web-interface")]
use super::rig_parser;
use super::time_utils;

/// LlamaIndex parsing API endpoint URL
pub const LLAMA_PARSING_ENDPOINT_EU: &str = "https://api.cloud.eu.llamaindex.ai/api/v1/";
pub const LLAMA_PARSING_ENDPOINT: &str = "https://api.cloud.eu.llamaindex.ai/api/v1/parsing/upload";

/// Response structure from LlamaIndex parsing service
#[derive(Debug, Deserialize)]
pub struct LlamaParsingJobResponse {
    pub id: String,
    pub status: String,
}

/// Job result structure from LlamaIndex
#[derive(Debug, Deserialize)]
pub struct LlamaJobResult {
    pub status: String,
    pub error_message: Option<String>,
}

/// Structured result from LlamaIndex
#[derive(Debug, Deserialize)]
pub struct LlamaStructuredResult {
    pub data: Value,
}

/// Raw result from LlamaIndex
#[derive(Debug, Deserialize)]
pub struct LlamaRawResult {
    pub raw_text: String,
}

/// Prompt for the LlamaIndex scheduling extraction
pub const PROMPT: &str = "**Strict Extraction Rules for output value:**

1.  **Primary Content Focus:** The output value MUST be the **primary content** visible in the cell corresponding to the specific employee and date. Many cells contain a primary entry (time, code, or text) written *above* a secondary numerical value (like 8, 7.5, 8.25, 8,5). **You MUST IGNORE the secondary numerical value below.** Use ONLY the primary entry found at the top/main part of the cell.
    *   *Example:* If a cell shows \"12-20.30\" on top and \"8,5\" below it, the correct output output is `\"12-20.30\"`.
    *   *Example:* If a cell shows \"vp\" on top and \"8\" below it, the correct output output is `\"vp\"`.

2.  **Time Ranges:** If the primary content is a time range (e.g., \"7-15\", \"12-20.30\", \"7.30-15.30\", \"8.35-16\", \"9-17L\"), use that exact string as the output value.
    *   **Normalize Commas:** If a time contains a comma, replace it with a period (e.g., \"7,30-15.30\" becomes \"7.30-15.30\").

3.  **Codes:** If the primary content is only a code (e.g., \"x\", \"v\", \"vv\", \"VL\", \"S\", \"tst\"), use that exact code string as the output value.

4.  **Text / Combined Entries:**
    *   If the primary content is text (e.g., \"Toive\", \"Palkat\", \"kuor\"), use that text string.
    *   If the primary content spans multiple lines within the main entry area (e.g., \"Pai-\" on one line and \"kalla\" below it), combine them into a single string with a hyphen or space as appropriate based on visual context (e.g., \"Pai-kalla\").
    *   If the primary content includes text combined with a code (e.g., \"Toive\" written above \"vp\"), represent this combination accurately, perhaps as \"Toive vp\". Capture the full primary content.

5.  **Empty Cells:** If the cell for a specific employee on a specific date is **visually blank or contains no primary text/code/time entry** (even if there might be a secondary number below, which should be ignored anyway per Rule 1), the output value MUST be an empty string `\"\"`. This applies to *any* day within the identified date range, including potentially empty days at the start, middle, or end of an employee's schedule row for the displayed period.

6.  **Accuracy:** Ensure correct mapping between the employee row, the date column, and the extracted cell content. Ignore the header rows/columns containing week numbers, day names, legends, and the final \"Yht.\" (total hours) column. Process *all* employees listed vertically in the first column.";

/// Parse a schedule image using LlamaIndex parsing service
pub async fn parse_schedule_image(
    employee_name: &str,
    image_data: &[u8],
) -> Result<WorkSchedule, String> {
    // Log the parsing action
    info!("Parsing schedule image for employee: {}", employee_name);
    info!("Image size: {} bytes", image_data.len());

    #[cfg(feature = "web-interface")]
    {
        // Use env to get API key for LlamaIndex
        let api_key = env::var("LLAMA_API_KEY")
            .map_err(|_| "LLAMA_API_KEY environment variable not set".to_string())?;

        // Create HTTP client
        let client = Client::new();

        // Create the multipart form
        let form = multipart::Form::new()
            .text("user_prompt", PROMPT)
            .text("premium_mode", "true")
            .text("add_system_prompt", "true")
            .text("disable_image_extraction", "true")
            .text("is_formatting_instruction", "true")
            .text("structured_output", "false")
            .text("disable_ocr", "false")
            .text("adaptive_long_table", "false")
            .text("annotate_links", "false")
            .text("do_not_unroll_columns", "false")
            .text("html_make_all_elements_visible", "false")
            .text("html_remove_navigation_elements", "false")
            .text("html_remove_fixed_elements", "false")
            .text("guess_xlsx_sheet_name", "false")
            .text("do_not_cache", "true")
            .text("invalidate_cache", "false")
            .text("output_pdf_of_document", "false")
            .text("take_screenshot", "false")
            .text("spreadsheet_extract_sub_tables", "false")
            .part(
                "file",
                multipart::Part::bytes(image_data.to_vec())
                    .file_name("schedule.jpg")
                    .mime_str("image/jpeg")
                    .map_err(|e| format!("Failed to create multipart form: {}", e))?,
            );

        // Make the request to upload the file to LlamaIndex
        let res = client
            .post(LLAMA_PARSING_ENDPOINT)
            .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
            .header(header::ACCEPT, "application/json")
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Failed to send request to LlamaIndex: {}", e))?;

        // Check if request was successful
        if !res.status().is_success() {
            let status = res.status();
            let error_body = res.text().await.unwrap_or_default();
            return Err(format!(
                "LlamaIndex parsing service returned error: Status {}, Body: {}",
                status, error_body
            ));
        }

        // Parse the response to get the job ID
        let response: LlamaParsingJobResponse = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        info!("Got status: {}", response.status);

        let job_id = response.id;
        info!("LlamaIndex job created with ID: {}", job_id);
        println!("Debug: LlamaIndex job created with ID: {}", job_id);

        // Poll for job completion
        let result = poll_job_until_complete(&client, &api_key, &job_id).await?;

        // Extract the data from the job result
        match result.status.as_str() {
            "completed" | "COMPLETED" | "SUCCESS" | "success" => {
                info!("LlamaIndex job completed successfully");

                // First, try to get the raw markdown result from LlamaIndex
                let markdown_url = format!(
                    "{}parsing/job/{}/result/raw/markdown",
                    LLAMA_PARSING_ENDPOINT_EU, job_id
                );
                debug!("Requesting markdown result from: {}", markdown_url);
                println!("Debug: Requesting markdown result from: {}", markdown_url);

                let markdown_res = client
                    .get(&markdown_url)
                    .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
                    .header(header::ACCEPT, "application/json")
                    .send()
                    .await;

                // If markdown endpoint succeeds, use it
                if let Ok(res) = markdown_res {
                    debug!("Markdown response status: {}", res.status());
                    println!("Debug: Markdown response status: {}", res.status());

                    if res.status().is_success() {
                        match res.text().await {
                            Ok(markdown_text) => {
                                info!("Successfully retrieved raw markdown result");
                                println!("Debug: Successfully retrieved raw markdown result");
                                debug!(
                                    "Markdown preview: {:.100}...",
                                    markdown_text.chars().take(100).collect::<String>()
                                );

                                // Process the markdown with Rig/Gemini directly
                                #[cfg(feature = "web-interface")]
                                {
                                    let current_year = Local::now().year();
                                    info!(
                                        "Processing markdown with Rig/Gemini for year {}",
                                        current_year
                                    );

                                    // Pass employee_name to rig_parser
                                    match rig_parser::parse_with_rig(
                                        &markdown_text,
                                        employee_name,
                                        current_year as u32,
                                    )
                                    .await
                                    {
                                        Ok(days) if !days.is_empty() => {
                                            info!("Successfully parsed schedule with Rig from markdown, found {} days", days.len());
                                            return convert_to_work_schedule(employee_name, days);
                                        }
                                        Ok(_) => {
                                            info!("Rig parser returned empty results from markdown, falling back to raw text");
                                        }
                                        Err(e) => {
                                            warn!("Rig parser failed with markdown: {}, falling back to raw text", e);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to parse markdown result: {}, falling back to raw text",
                                    e
                                );
                            }
                        }
                    } else {
                        warn!("Failed to get markdown result with status {}, falling back to raw text", res.status());
                    }
                }

                // Fall back to raw text result if markdown fails or is not usable
                let raw_url = format!("{}parsing/{}/result/raw", LLAMA_PARSING_ENDPOINT_EU, job_id);
                debug!("Requesting raw text result from: {}", raw_url);
                println!("Debug: Requesting raw text result from: {}", raw_url);

                let raw_res = client
                    .get(&raw_url)
                    .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
                    .header(header::ACCEPT, "application/json")
                    .send()
                    .await
                    .map_err(|e| format!("Failed to get raw result: {}", e))?;

                if !raw_res.status().is_success() {
                    let status = raw_res.status();
                    let error_body = raw_res.text().await.unwrap_or_default();
                    return Err(format!(
                        "Failed to get raw result: Status {}, Body: {}",
                        status, error_body
                    ));
                }

                let raw_result: LlamaRawResult = raw_res
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse raw result: {}", e))?;

                // Process the raw text with Rig/Gemini directly
                #[cfg(feature = "web-interface")]
                {
                    let current_year = Local::now().year() as u32;
                    info!(
                        "Processing raw text with Rig/Gemini for year {}",
                        current_year
                    );

                    // Pass employee_name to rig_parser
                    match rig_parser::parse_with_rig(
                        &raw_result.raw_text,
                        employee_name,
                        current_year,
                    )
                    .await
                    {
                        Ok(days) if !days.is_empty() => {
                            info!(
                                "Successfully parsed schedule with Rig, found {} days",
                                days.len()
                            );
                            return convert_to_work_schedule(employee_name, days);
                        }
                        Ok(_) => {
                            info!("Rig parser returned empty results, falling back to structured JSON parsing");
                        }
                        Err(e) => {
                            warn!(
                                "Rig parser failed: {}, falling back to structured JSON parsing",
                                e
                            );
                        }
                    }
                }

                // If Rig processing fails or is not available, try to extract structured JSON
                let structured_url = format!(
                    "{}parsing/{}/result/structured",
                    LLAMA_PARSING_ENDPOINT_EU, job_id
                );
                debug!("Requesting structured result from: {}", structured_url);
                println!(
                    "Debug: Requesting structured result from: {}",
                    structured_url
                );

                let structured_res = client
                    .get(&structured_url)
                    .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
                    .header(header::ACCEPT, "application/json")
                    .send()
                    .await
                    .map_err(|e| format!("Failed to get structured result: {}", e))?;

                if !structured_res.status().is_success() {
                    let status = structured_res.status();
                    let error_body = structured_res.text().await.unwrap_or_default();
                    return Err(format!(
                        "Failed to get structured result: Status {}, Body: {}",
                        status, error_body
                    ));
                }

                let result: LlamaStructuredResult = structured_res
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse structured result: {}", e))?;

                // Extract the work days from the JSON result
                let json_str = result.data.to_string();

                // Parse the JSON data into our extraction format
                let extracted_days = extract_json_array(&json_str)
                    .map_err(|e| format!("Failed to extract JSON from result: {}", e))?;

                convert_to_work_schedule(employee_name, extracted_days)
            }
            "failed" => {
                let error_msg = result
                    .error_message
                    .unwrap_or_else(|| "Unknown error".to_string());
                Err(format!("LlamaIndex job failed: {}", error_msg))
            }
            status => Err(format!("Unexpected job status: {}", status)),
        }
    }
    #[cfg(not(feature = "web-interface"))]
    {
        // For non-web-interface builds, just return a mock schedule
        super::mock_parse_schedule(employee_name)
    }
}

/// Poll the LlamaIndex job until it completes or fails
pub async fn poll_job_until_complete(
    client: &Client,
    api_key: &str,
    job_id: &str,
) -> Result<LlamaJobResult, String> {
    const MAX_POLLS: u32 = 60;
    const POLL_DELAY_MS: u64 = 1000;

    let job_url = format!("{}parsing/job/{}", LLAMA_PARSING_ENDPOINT_EU, job_id);
    debug!("Polling job status from: {}", job_url);
    println!("Debug: Polling job status from: {}", job_url);

    for attempt in 1..=MAX_POLLS {
        let res = client
            .get(&job_url)
            .header(header::AUTHORIZATION, format!("Bearer {}", api_key))
            .header(header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|e| format!("Failed to poll job status: {}", e))?;

        if !res.status().is_success() {
            let status = res.status();
            let error_body = res.text().await.unwrap_or_default();
            return Err(format!(
                "Failed to get job status: Status {}, Body: {}",
                status, error_body
            ));
        }

        let job_result: LlamaJobResult = res
            .json()
            .await
            .map_err(|e| format!("Failed to parse job status: {}", e))?;

        match job_result.status.as_str() {
            "completed" | "COMPLETED" | "SUCCESS" | "success" | "failed" | "FAILED" => {
                println!("Debug: Job status final: {}", job_result.status);
                return Ok(job_result);
            }
            "processing" | "PROCESSING" | "pending" | "PENDING" => {
                info!(
                    "Job status: {}, poll attempt {}/{}",
                    job_result.status, attempt, MAX_POLLS
                );
                println!(
                    "Debug: Job status: {}, poll attempt {}/{}",
                    job_result.status, attempt, MAX_POLLS
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(POLL_DELAY_MS)).await;
            }
            status => {
                warn!("Unknown job status: {}", status);
                println!("Debug: Unknown job status: {}", status);
                tokio::time::sleep(tokio::time::Duration::from_millis(POLL_DELAY_MS)).await;
            }
        }
    }

    Err(format!(
        "Job polling timed out after {} attempts",
        MAX_POLLS
    ))
}

/// Extract JSON array from text
pub fn extract_json_array(text: &str) -> Result<Vec<WorkDayExtraction>, String> {
    // Try to parse the text directly as a JSON array
    match from_str::<Vec<WorkDayExtraction>>(text) {
        Ok(days) => Ok(days),
        Err(e) => {
            // If direct parsing fails, try to find and extract a JSON array in the text
            warn!("Failed to parse JSON directly: {}", e);

            // Try to find a JSON array in the text (between [ and ])
            if let (Some(start_idx), Some(end_idx)) = (text.find('['), text.rfind(']')) {
                if start_idx < end_idx {
                    let json_str = &text[start_idx..=end_idx];
                    match from_str::<Vec<WorkDayExtraction>>(json_str) {
                        Ok(days) => Ok(days),
                        Err(e) => Err(format!("Failed to parse extracted JSON array: {}", e)),
                    }
                } else {
                    Err("Invalid JSON array structure".to_string())
                }
            } else {
                Err("No JSON array found in response".to_string())
            }
        }
    }
}

/// Convert extracted work days to a WorkSchedule
pub fn convert_to_work_schedule(
    employee_name: &str,
    extracted_days: Vec<WorkDayExtraction>,
) -> Result<WorkSchedule, String> {
    let mut schedule = WorkSchedule::new(employee_name.to_string());

    debug!("Extracted days: {:?}", extracted_days);

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
                    let start_time = time_utils::normalize_time(parts[0]);
                    let end_time = time_utils::normalize_time(parts[1]);

                    schedule.add_day(WorkDay {
                        date: day.date,
                        start_time: Some(start_time),
                        end_time: Some(end_time),
                        is_day_off: false,
                        notes: None,
                    });
                } else {
                    // Cannot parse time range, treat as note
                    schedule.add_day(WorkDay {
                        date: day.date,
                        start_time: None,
                        end_time: None,
                        is_day_off: false,
                        notes: Some(day.work_hours),
                    });
                }
            } else if day.work_hours.to_lowercase() == "x" {
                // Day off
                schedule.add_day(WorkDay {
                    date: day.date,
                    start_time: None,
                    end_time: None,
                    is_day_off: true,
                    notes: None,
                });
            } else {
                // Treat as note
                schedule.add_day(WorkDay {
                    date: day.date,
                    start_time: None,
                    end_time: None,
                    is_day_off: false,
                    notes: Some(day.work_hours),
                });
            }
        }
    }

    Ok(schedule)
}
