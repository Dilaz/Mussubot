use crate::model::WorkDayExtraction;
use rig::completion::{Chat, Message};
use rig::providers::gemini::Client as GeminiClient;
use serde_json::from_str;
use std::env;
use tracing::{error, info};

const SYSTEM_PROMPT: &str = "You are a work schedule parser. You need to analyze the given text that describes work schedules and extract dates and work hours information. Output your findings as a JSON array with each entry containing a date and work_hours fields.";

const USER_PROMPT_TEMPLATE: &str = "Analyze the provided text which contains a work schedule in a table-like format.
Identify the row corresponding to the employee named {name}.

The schedule covers a specific period (e.g., March 3rd to March 23rd, {year}). The columns represent specific dates, often indicated by D.M. format (e.g., 3.3., 4.3., ..., 23.3.) within the header row. Assume the year is {year} unless otherwise specified in the header.

For the identified employee {name}, iterate through each column that represents a specific date within the schedule's range.
1.  Extract the date from the column header. Convert the D.M. format to YYYY-MM-DD format using the year {year}. Handle single-digit days and months by padding with a zero (e.g., 3.3. becomes {year}-03-03, 10.3. becomes {year}-03-10).
2.  Extract the corresponding value from the cell in the employee's row for that specific date column. This value represents the work assignment or status for that day (e.g., '7-15', 'v', 'x', 'S', 'Pai-kalla', 'Toive vp', empty).

Generate a JSON array containing objects for each day's entry found for {name}. Each object must adhere *strictly* to the following format:
[{
  \"date\": \"YYYY-MM-DD\",
  \"work_hours\": \"VALUE_FROM_CELL\"
}]

Do not include columns that do not represent a specific date (like week numbers, descriptive headers like 'Työtunnit', 'Yht.').
Ensure the output contains *only* the JSON array structure as specified. Do not include any introductory text, explanations, variable assignments, or any other text outside the JSON array in your response. The response must start with `[` and end with `]`.

Text to parse:
{markdown}";

/// Parse markdown text using Rig with Google Gemini
pub async fn parse_with_rig(
    markdown: &str,
    name: &str,
    year: u32,
) -> Result<Vec<WorkDayExtraction>, String> {
    info!("Parsing markdown with Rig and Google Gemini");

    // Get API key from environment variable
    let api_key = env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY environment variable not set".to_string())?;

    // Get model name from environment variable
    let model = env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-pro-exp-03-25".to_string());
    info!("Using Gemini model: {}", model);

    // Initialize Gemini client with API key
    let gemini_client = GeminiClient::new(&api_key);

    // Prepare the prompt for Gemini
    let user_prompt = USER_PROMPT_TEMPLATE
        .replace("{name}", name)
        .replace("{year}", &year.to_string())
        .replace("{markdown}", markdown);

    // Create chat messages
    let agent = gemini_client
        .agent(&model)
        .preamble(SYSTEM_PROMPT)
        .temperature(0.2)
        .build();

    // Make the request to Gemini directly using the existing runtime
    let response = agent
        .chat(user_prompt, Vec::<Message>::new())
        .await
        .map_err(|e| format!("Rig API request failed: {}", e))?;

    // Get the response content
    info!("Received response from Gemini");

    // Attempt to parse JSON from the response
    parse_json_from_response(&response)
}

/// Attempt to parse JSON from the model response
fn parse_json_from_response(response: &str) -> Result<Vec<WorkDayExtraction>, String> {
    // Try to extract JSON array from the text
    if let Some(json_start) = response.find('[') {
        if let Some(json_end) = response.rfind(']') {
            if json_start < json_end {
                let json_str = &response[json_start..=json_end];
                match from_str::<Vec<WorkDayExtraction>>(json_str) {
                    Ok(days) => return Ok(days),
                    Err(e) => {
                        error!("Failed to parse JSON from response: {}", e);
                        error!("JSON string: {}", json_str);
                    }
                }
            }
        }
    }

    // Try to parse the entire response as JSON (in case it's already clean JSON)
    match from_str::<Vec<WorkDayExtraction>>(response) {
        Ok(days) => return Ok(days),
        Err(e) => {
            error!("Failed to parse entire response as JSON: {}", e);
        }
    }

    // Could not extract valid JSON
    error!("Could not extract valid JSON from response: {}", response);
    Err("Could not extract valid JSON from the model response".to_string())
}
