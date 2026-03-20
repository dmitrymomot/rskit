use std::time::Duration;

use chrono::{DateTime, Utc};

pub(crate) enum Schedule {
    Cron(Box<croner::Cron>),
    Interval(Duration),
}

impl Schedule {
    pub(crate) fn parse(input: &str) -> Self {
        let trimmed = input.trim();

        // Named aliases
        match trimmed {
            "@yearly" | "@annually" => return Self::parse_cron("0 0 0 1 1 *"),
            "@monthly" => return Self::parse_cron("0 0 0 1 * *"),
            "@weekly" => return Self::parse_cron("0 0 0 * * 0"),
            "@daily" | "@midnight" => return Self::parse_cron("0 0 0 * * *"),
            "@hourly" => return Self::parse_cron("0 0 * * * *"),
            _ => {}
        }

        // @every duration
        if let Some(dur_str) = trimmed.strip_prefix("@every ") {
            let duration = parse_duration(dur_str.trim());
            return Self::Interval(duration);
        }

        // Standard cron expression
        Self::parse_cron(trimmed)
    }

    fn parse_cron(expr: &str) -> Self {
        let cron = croner::Cron::new(expr)
            .parse()
            .unwrap_or_else(|e| panic!("invalid cron expression '{expr}': {e}"));
        Self::Cron(Box::new(cron))
    }

    pub(crate) fn next_tick(&self, from: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Self::Cron(cron) => cron
                .find_next_occurrence(&from, false)
                .expect("cron expression has no future occurrence"),
            Self::Interval(dur) => from + chrono::Duration::from_std(*dur).unwrap(),
        }
    }
}

fn parse_duration(s: &str) -> Duration {
    let mut total_secs: u64 = 0;
    let mut current_num = String::new();
    let mut found_any = false;

    for ch in s.chars() {
        match ch {
            '0'..='9' => current_num.push(ch),
            'h' => {
                let n: u64 = current_num
                    .parse()
                    .unwrap_or_else(|_| panic!("invalid duration '{s}': bad number before 'h'"));
                total_secs += n * 3600;
                current_num.clear();
                found_any = true;
            }
            'm' => {
                let n: u64 = current_num
                    .parse()
                    .unwrap_or_else(|_| panic!("invalid duration '{s}': bad number before 'm'"));
                total_secs += n * 60;
                current_num.clear();
                found_any = true;
            }
            's' => {
                let n: u64 = current_num
                    .parse()
                    .unwrap_or_else(|_| panic!("invalid duration '{s}': bad number before 's'"));
                total_secs += n;
                current_num.clear();
                found_any = true;
            }
            _ => panic!("invalid duration '{s}': unexpected character '{ch}'"),
        }
    }

    if !current_num.is_empty() {
        panic!("invalid duration '{s}': trailing number without unit (use h, m, or s)");
    }

    if !found_any {
        panic!("invalid duration '{s}': no duration components found");
    }

    Duration::from_secs(total_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_hours() {
        assert_eq!(parse_duration("2h"), Duration::from_secs(7200));
    }

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration("15m"), Duration::from_secs(900));
    }

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("30s"), Duration::from_secs(30));
    }

    #[test]
    fn parse_duration_combined() {
        assert_eq!(parse_duration("1h30m"), Duration::from_secs(5400));
        assert_eq!(parse_duration("2h30m15s"), Duration::from_secs(9015));
    }

    #[test]
    #[should_panic(expected = "invalid duration")]
    fn parse_duration_rejects_days() {
        parse_duration("1d");
    }

    #[test]
    #[should_panic(expected = "invalid duration")]
    fn parse_duration_rejects_ms() {
        parse_duration("500ms");
    }

    #[test]
    #[should_panic(expected = "trailing number without unit")]
    fn parse_duration_rejects_bare_number() {
        parse_duration("30");
    }

    #[test]
    #[should_panic(expected = "no duration components")]
    fn parse_duration_rejects_empty() {
        parse_duration("");
    }
}
