use crate::model::WorkDayExtraction;
use base64::{self, engine::Engine};
use rig::completion::{Chat, Message};
use rig::message::{ContentFormat, Image, ImageMediaType};
use rig::providers::gemini::Client as GeminiClient;
use serde_json::from_str;
use std::env;
use tracing::{error, info};

const SYSTEM_PROMPT: &str = "You are a work schedule parser. You need to analyze the given text that describes work schedules and extract dates and work hours information. Output your findings as a JSON array with each entry containing a date and work_hours fields.";

const USER_PROMPT_TEMPLATE: &str = r#"**Task:** Generate Employee Schedule JSON Directly from Image, Using Provided Markdown as a Guide

**Context:**
You are provided with:
1.  An original work schedule **image**. This is the **ABSOLUTE SOURCE OF TRUTH**.
2.  A **preliminary markdown table** (provided at the end of this prompt). This table was extracted previously and **is known to CONTAIN ERRORS**. Use it **only** as a navigational aid.
3.  A **Target Employee** name (`[EMPLOYEE_NAME]`).
4.  The relevant **Year** (`[YEAR]`).

**Your Goal:**
Your **only goal** is to produce a **perfectly accurate** JSON schedule for the **Target Employee** by extracting data **exclusively from the IMAGE**. The provided markdown table is **unreliable and contains errors**; it is for guidance only. **NEVER** copy data from the markdown table.

**Extraction Process:**

1.  **Identify Target Row in Image:** Locate the row corresponding to the **Target Employee** (`[EMPLOYEE_NAME]`) **in the provided image**. You can use the preliminary markdown table below to help find the correct row visually, but the image is your reference.
2.  **Identify Date Columns in Image:** Identify the date columns relevant to the schedule period shown **in the image headers** (e.g., '14.4.', '15.4.', '16.4.', etc.).
3.  **Direct Image Extraction per Cell:** For each relevant date column identified in step 2:
    *   **Locate the specific cell** in the **IMAGE** at the intersection of the Target Employee's row and the current date column.
    *   **Analyze the Image Cell Content:** Apply the **Strict Extraction Rules** (see below) to the **primary content** visible within this specific image cell.
    *   **Determine the `work_hours` Value:** The value must come **only** from the image cell. **NEVER** use the value from the preliminary markdown table, even if the image cell is empty. If the markdown shows data but the image is blank, the correct output is an empty string `""`.
    *   **Format Date:** Combine the date from the image column header (e.g., '14.4.') with the `[YEAR]` to create the "YYYY-MM-DD" format (e.g., "[YEAR]-04-14").
    *   **Create JSON Object:** Construct a JSON object: `{ "date": "YYYY-MM-DD", "work_hours": "IMAGE_EXTRACTED_VALUE" }`.
4.  **Compile JSON Array:** Collect all the generated JSON objects for the employee into a single JSON array.

**Strict Extraction Rules (Apply ONLY to the IMAGE):**

1.  **Primary Content is King:** Extract *only* the main content written in the upper/primary part of the image cell.
2.  **IGNORE Secondary Bottom Number:** **CRITICAL:** Completely ignore any smaller number (like `8`, `7.5`, `8,25`) written *below* the primary entry in the image cell. Do not include it in the `work_hours` value.
3.  **Exactness is Required:**
    *   Extract times, codes, and text *exactly* as they appear in the primary area of the image cell.
    *   Treat variations like "9-17" and "9-17L" as distinct entries if they appear that way in different image cells.
    *   Normalize comma decimals in times to periods (e.g., image "7,30-..." becomes "7.30-...").
4.  **Capture ALL Entries (Including Single Letters):** If the primary content in the image cell is a code (e.g., "x", "v", "vv", "VL", "S", "tst", "vp"), that code *is* the value. **Ensure single characters like "v" or "x" are captured.**
5.  **Text and Combinations:** Extract text ("Toive"), combined lines ("Pai-kalla" -> "Paikalla"), or combined text/codes ("Toive vp") precisely as seen in the primary area of the image cell.
6.  **Visually Blank Cells:** If the primary content area of the **image cell** is visually empty for the target employee on a specific date, the extracted `work_hours` value MUST be an empty string `""`.

**Critical Instructions on a Flawed Guide (The Markdown Table):**
*   The markdown table is known to be **unreliable and contain multiple errors**.
*   Its **only** purpose is to help you find the employee's name and the general layout of the dates.
*   **You MUST IGNORE the data within the markdown table's cells.**
*   Every single `work_hours` value you output MUST be the result of a fresh analysis of the corresponding cell **in the IMAGE**.
*   If the image and markdown disagree, the **IMAGE IS ALWAYS RIGHT**. There are no exceptions. Trust the image, not the text.

**JSON Output Format:**
*   Output MUST be a single JSON array `[...]`.
*   Each element MUST be an object: `{ "date": "YYYY-MM-DD", "work_hours": "value_extracted_from_image" }`.
*   Include an object for every relevant date column processed for the employee.

**Example Object:**
```json
{
  "date": "[YEAR]-04-17",
  "work_hours": "v"
}
```

**Strict Output Constraint:**
Provide **only** the final JSON array as the output. Do not include any introductory text, explanations, markdown formatting (like `json ...`), or comments before or after the JSON data. Just the raw JSON array string starting with `[` and ending with `]`.

Preliminary Markdown Table (Use for Navigation/Guidance Only - May Contain Errors):

```markdown
[PRELIMINARY_MARKDOWN]
```
"#;

const MARKDOWN_PLACEHOLDER: &str = "[PRELIMINARY_MARKDOWN]";
const NAME_PLACEHOLDER: &str = "[EMPLOYEE_NAME]";
const YEAR_PLACEHOLDER: &str = "[YEAR]";

/// Parse markdown and image using Rig with Google Gemini
pub async fn parse_with_rig(
    image_data: &[u8],
    markdown: &str,
    name: &str,
    year: u32,
) -> Result<Vec<WorkDayExtraction>, String> {
    info!("Parsing work schedule with Rig and Google Gemini");

    // Get API key from environment variable
    let api_key = env::var("GEMINI_API_KEY")
        .map_err(|_| "GEMINI_API_KEY environment variable not set".to_string())?;

    // Get model name from environment variable
    let model = env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-pro".to_string());
    info!("Using Gemini model: {}", model);

    // Initialize Gemini client with API key
    let gemini_client = GeminiClient::new(&api_key);

    // Base64 encode the image
    let base64_image = base64::engine::general_purpose::STANDARD.encode(image_data);

    let image = Image {
        data: base64_image,
        media_type: Some(ImageMediaType::JPEG),
        format: Some(ContentFormat::Base64),
        detail: None,
    };
    let messages = vec![Message::from(image)];

    // Prepare the prompt for Gemini
    let user_prompt = USER_PROMPT_TEMPLATE
        .replace(MARKDOWN_PLACEHOLDER, markdown)
        .replace(NAME_PLACEHOLDER, name)
        .replace(YEAR_PLACEHOLDER, &year.to_string());

    // Create chat messages
    let agent = gemini_client
        .agent(&model)
        .preamble(SYSTEM_PROMPT)
        .temperature(0.0)
        .build();

    // Make the request to Gemini directly using the existing runtime
    let response = agent
        .chat(user_prompt, messages)
        .await
        .map_err(|e| format!("Rig API request failed: {e}"))?;

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
