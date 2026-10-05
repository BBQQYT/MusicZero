//! CLI time parsing and bounded seek targets shared by IPC and MPRIS.
use crate::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeekRequest {
    Absolute(i64),
    Relative(i64),
}

impl SeekRequest {
    pub fn parse(value: &str) -> Result<Self> {
        let (relative, negative, time) = match value.as_bytes().first() {
            Some(b'+') => (true, false, &value[1..]),
            Some(b'-') => (true, true, &value[1..]),
            _ => (false, false, value),
        };
        let parts: Vec<_> = time.split(':').collect();
        if time.len() > 64 || parts.is_empty() || parts.len() > 3 {
            return Err(
                "Укажите секунды, MM:SS или HH:MM:SS; +/− для относительной перемотки".into(),
            );
        }
        let seconds = *parts.last().unwrap();
        let (whole, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
        let number = |part: &str| -> Result<u64> {
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err("Некорректное время перемотки".into());
            }
            Ok(part.parse()?)
        };
        let whole = number(whole)?;
        if fraction.len() > 6
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            || (seconds.contains('.') && fraction.is_empty())
        {
            return Err("Дробная часть времени должна содержать от 1 до 6 цифр".into());
        }
        if parts.len() > 1 && whole >= 60 {
            return Err("Секунды в MM:SS/HH:MM:SS должны быть меньше 60".into());
        }
        let mut total = whole;
        for (index, part) in parts[..parts.len() - 1].iter().rev().enumerate() {
            let amount = number(part)?;
            if parts.len() == 3 && index == 0 && amount >= 60 {
                return Err("Минуты в HH:MM:SS должны быть меньше 60".into());
            }
            total = amount
                .checked_mul(if index == 0 { 60 } else { 3600 })
                .and_then(|amount| total.checked_add(amount))
                .ok_or("Время слишком большое")?;
        }
        let fraction = if fraction.is_empty() {
            0
        } else {
            number(fraction)? * 10u64.pow(6 - fraction.len() as u32)
        };
        let micros = total
            .checked_mul(1_000_000)
            .and_then(|v| v.checked_add(fraction))
            .and_then(|v| i64::try_from(v).ok())
            .ok_or("Время слишком большое")?;
        Ok(if relative {
            Self::Relative(if negative { -micros } else { micros })
        } else {
            Self::Absolute(micros)
        })
    }

    pub fn from_wire(value: &str, mode: &str) -> Result<Self> {
        let micros = value.parse::<i64>()?;
        match mode {
            "relative" => Ok(Self::Relative(micros)),
            "absolute" if micros >= 0 => Ok(Self::Absolute(micros)),
            _ => Err("Некорректный режим или позиция перемотки".into()),
        }
    }

    pub fn wire(self) -> (String, &'static str) {
        match self {
            Self::Relative(micros) => (micros.to_string(), "relative"),
            Self::Absolute(micros) => (micros.to_string(), "absolute"),
        }
    }

    pub fn target(self, current_us: i64, duration_us: i64) -> u64 {
        let target = match self {
            Self::Relative(offset) => current_us.saturating_add(offset),
            Self::Absolute(position) => position,
        }
        .max(0);
        // Zero means unknown duration, not a zero-length finite track.
        if duration_us > 0 {
            target.min(duration_us) as u64
        } else {
            target as u64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_seconds_and_clock_times_preserve_sign_and_fraction() {
        for (text, expected) in [
            ("90", SeekRequest::Absolute(90_000_000)),
            ("1:30.25", SeekRequest::Absolute(90_250_000)),
            ("01:02:03", SeekRequest::Absolute(3_723_000_000)),
            ("+10", SeekRequest::Relative(10_000_000)),
            ("-0.5", SeekRequest::Relative(-500_000)),
            ("+1:00", SeekRequest::Relative(60_000_000)),
        ] {
            let parsed = SeekRequest::parse(text).unwrap();
            assert_eq!(parsed, expected);
            let (value, mode) = parsed.wire();
            assert_eq!(SeekRequest::from_wire(&value, mode).unwrap(), expected);
        }
    }

    #[test]
    fn invalid_or_overflowing_times_are_rejected() {
        for text in [
            "",
            "+",
            "--1",
            "NaN",
            "inf",
            "1e2",
            "1:60",
            "1:60:00",
            "1:2:3:4",
            "1.",
            "0.0000001",
            "-1:-2",
            " 5",
            "18446744073709551616",
            "9223372036855",
        ] {
            assert!(SeekRequest::parse(text).is_err(), "{text}");
        }
        assert!(SeekRequest::from_wire("-1", "absolute").is_err());
        assert!(SeekRequest::from_wire("1", "invalid").is_err());
    }

    #[test]
    fn targets_clamp_known_duration_but_allow_unknown_duration() {
        assert_eq!(SeekRequest::Relative(-20).target(10, 100), 0);
        assert_eq!(SeekRequest::Absolute(200).target(10, 100), 100);
        assert_eq!(SeekRequest::Absolute(200).target(10, 0), 200);
        assert_eq!(
            SeekRequest::Relative(i64::MAX).target(10, 0),
            i64::MAX as u64
        );
    }
}
