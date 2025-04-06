mod llamaindex;
#[cfg(feature = "web-interface")]
mod rig_parser;
mod time_utils;

pub use llamaindex::parse_schedule_image;

#[cfg(not(feature = "web-interface"))]
pub fn mock_parse_schedule(employee_name: &str) -> Result<WorkSchedule, String> {
    info!("Using mock schedule data for {}", employee_name);

    let mut schedule = WorkSchedule::new(employee_name.to_string());

    // Generate a 2-week schedule starting from tomorrow
    let tomorrow = Local::now().date_naive() + Duration::days(1);

    for i in 0..14 {
        let date = tomorrow + Duration::days(i);
        let date_str = date.format("%Y-%m-%d").to_string();

        // Generate different entries based on the day of the week
        match date.weekday().num_days_from_monday() {
            // Weekend (Saturday and Sunday)
            5 | 6 => {
                schedule.add_day(WorkDay {
                    date: date_str,
                    start_time: None,
                    end_time: None,
                    is_day_off: true,
                    notes: None,
                });
            }
            // Monday, Wednesday, Friday
            0 | 2 | 4 => {
                schedule.add_day(WorkDay {
                    date: date_str,
                    start_time: Some(format!("08:00")),
                    end_time: Some(format!("16:00")),
                    is_day_off: false,
                    notes: None,
                });
            }
            // Tuesday, Thursday
            1 | 3 => {
                schedule.add_day(WorkDay {
                    date: date_str,
                    start_time: Some(format!("12:00")),
                    end_time: Some(format!("20:00")),
                    is_day_off: false,
                    notes: None,
                });
            }
            _ => unreachable!(),
        }
    }

    Ok(schedule)
}
