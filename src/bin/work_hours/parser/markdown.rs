use crate::model::WorkDayExtraction;
use chrono::{Datelike, Local, NaiveDate};
use serde_json::from_str;
use tracing::{info, warn};

#[cfg(feature = "web-interface")]
use super::rig_parser;

/// Parse the markdown response to extract the work schedule
pub fn parse_markdown_schedule(markdown: &str, employee_name: Option<&str>) -> Result<Vec<WorkDayExtraction>, String> {
    info!("Parsing markdown schedule");
    
    #[cfg(feature = "web-interface")]
    {
        // Get current year
        let current_year = Local::now().year() as u32;
        
        // Try using Rig parser first
        let name = employee_name.unwrap_or("unknown");
        match rig_parser::parse_with_rig(markdown, name, current_year) {
            Ok(days) if !days.is_empty() => {
                info!("Successfully parsed schedule with Rig, found {} days", days.len());
                return Ok(days);
            }
            Ok(_) => {
                info!("Rig parser returned empty results, falling back to traditional parsing");
            }
            Err(e) => {
                warn!("Rig parser failed: {}, falling back to traditional parsing", e);
            }
        }
    }
    
    // Fallback to traditional parsing
    
    let mut extracted_days = Vec::new();
    
    // Try to find JSON in the text first
    if let Ok(days) = extract_json_from_markdown(markdown) {
        if !days.is_empty() {
            info!("Found JSON data in markdown with {} entries", days.len());
            return Ok(days);
        }
    }
    
    // If no JSON found, try to parse markdown tables
    let mut table_found = false;
    let mut header_row;
    let mut date_indices = Vec::new();
    
    for line in markdown.lines() {
        let trimmed = line.trim();
        
        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }
        
        // Check if this is a table line (starts with |)
        if trimmed.starts_with('|') {
            // Split the line by | and trim each cell
            let cells: Vec<&str> = trimmed
                .split('|')
                .map(|s| s.trim())
                .collect();
            
            // Skip separator lines (like |---|---|)
            if cells.iter().any(|cell| cell.contains("---")) {
                continue;
            }
            
            // If this is the first table line, treat it as header
            if !table_found {
                table_found = true;
                header_row = cells;
                
                // Find date columns in the header
                for (i, cell) in header_row.iter().enumerate() {
                    if let Ok(date) = parse_date_from_header(cell) {
                        date_indices.push((i, date));
                    }
                }
                
                info!("Found {} date columns in table header", date_indices.len());
                continue;
            }
            
            // Process data rows
            for (col_idx, date) in &date_indices {
                if *col_idx < cells.len() {
                    let work_hours = cells[*col_idx].trim();
                    
                    // Only add non-empty entries
                    if !work_hours.is_empty() {
                        extracted_days.push(WorkDayExtraction {
                            date: date.clone(),
                            work_hours: work_hours.to_string(),
                        });
                    }
                }
            }
        }
    }
    
    info!("Extracted {} days from markdown table", extracted_days.len());
    Ok(extracted_days)
}

/// Extract JSON data from markdown text
pub fn extract_json_from_markdown(markdown: &str) -> Result<Vec<WorkDayExtraction>, String> {
    // Look for code blocks that might contain JSON
    let json_pattern = r"```(?:json)?\s*(\[.*?\])\s*```";
    let re = Regex::new(json_pattern)
        .map_err(|e| format!("Failed to compile regex: {}", e))?;
    
    for cap in re.captures_iter(markdown) {
        if let Some(json_str) = cap.get(1) {
            match from_str::<Vec<WorkDayExtraction>>(json_str.as_str()) {
                Ok(days) => return Ok(days),
                Err(e) => {
                    warn!("Failed to parse JSON from code block: {}", e);
                    continue;
                }
            }
        }
    }
    
    // If no JSON code blocks, look for arrays in the text
    if let (Some(start_idx), Some(end_idx)) = (markdown.find('['), markdown.rfind(']')) {
        if start_idx < end_idx {
            let json_str = &markdown[start_idx..=end_idx];
            match from_str::<Vec<WorkDayExtraction>>(json_str) {
                Ok(days) => return Ok(days),
                Err(e) => {
                    warn!("Failed to parse JSON from text: {}", e);
                }
            }
        }
    }
    
    // No valid JSON found
    Ok(Vec::new())
}

/// Parse a date from a table header cell
pub fn parse_date_from_header(header: &str) -> Result<String, String> {
    // Handle various date formats that might appear in headers
    // Common formats: "2023-04-01", "1.4.2023", "1.4.", etc.
    
    // First try ISO format YYYY-MM-DD
    if let Ok(date) = NaiveDate::parse_from_str(header, "%Y-%m-%d") {
        return Ok(date.format("%Y-%m-%d").to_string());
    }
    
    // Try Finnish format with year (DD.MM.YYYY)
    if let Ok(date) = NaiveDate::parse_from_str(header, "%d.%m.%Y") {
        return Ok(date.format("%Y-%m-%d").to_string());
    }
    
    // Try Finnish format without year (DD.MM.)
    let current_year = Local::now().year();
    let with_year = format!("{}.{}", header, current_year);
    if let Ok(date) = NaiveDate::parse_from_str(&with_year, "%d.%m.%Y") {
        return Ok(date.format("%Y-%m-%d").to_string());
    }
    
    // Check if it contains any digits (might be a date with day name)
    if header.chars().any(|c| c.is_digit(10)) {
        // Extract digits and try to parse
        let digits: String = header.chars().filter(|c| c.is_digit(10) || *c == '.').collect();
        
        // Try parsing with assumption that it's a day + month
        if digits.contains('.') {
            let parts: Vec<&str> = digits.split('.').collect();
            if parts.len() >= 2 {
                if let (Ok(day), Ok(month)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                    if day >= 1 && day <= 31 && month >= 1 && month <= 12 {
                        let year = Local::now().year();
                        if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                            return Ok(date.format("%Y-%m-%d").to_string());
                        }
                    }
                }
            }
        }
    }
    
    Err(format!("Could not parse date from header: {}", header))
} 