#[cfg(feature = "spice")]
use chrono::Utc;
use reqwest::blocking::Client;
#[cfg(feature = "spice")]
use spice::SpiceLock;
use std::path::Path;
#[cfg(feature = "spice")]
use std::sync::Mutex;
use std::time::Duration;

#[cfg(feature = "spice")]
const SECONDS_PER_DAY: f64 = 86_400.0;
const KM_PER_AU: f64 = 149_597_870.7;
const MOON_SEMI_MAJOR_AXIS_KM: f64 = 384_400.0;
const CHARON_SEMI_MAJOR_AXIS_KM: f64 = 19_591.0;
#[cfg(feature = "spice")]
const SPICE_REFERENCE_FRAME: &str = "ECLIPJ2000";
const HORIZONS_API_URL: &str = "https://ssd.jpl.nasa.gov/api/horizons.api";
const HORIZONS_CENTER: &str = "'500@10'";

#[derive(Clone, Copy)]
struct FallbackOrbit {
    semi_major_axis_au: f64,
    period_days: f64,
    phase_radians: f64,
    inclination_radians: f64,
}

#[cfg(feature = "spice")]
enum EphemerisState {
    #[cfg(feature = "spice")]
    Spice {
        lock: Mutex<SpiceLock>,
        base_et: f64,
    },
    Fallback,
}

pub struct SpiceEphemeris {
    #[cfg(feature = "spice")]
    state: EphemerisState,
    status_line: String,
}

impl SpiceEphemeris {
    #[cfg(feature = "spice")]
    pub fn new(spice_dir: &Path) -> Self {
        Self::new_with_spice(spice_dir)
    }

    #[cfg(not(feature = "spice"))]
    pub fn new(spice_dir: &Path) -> Self {
        let _ = spice_dir;
        Self {
            status_line: "Fallback orbit mode active: app was compiled without the `spice` feature"
                .to_string(),
        }
    }

    #[cfg(feature = "spice")]
    fn new_with_spice(spice_dir: &Path) -> Self {
        let leap_seconds = spice_dir.join("naif0012.tls");
        let planetary_ephemeris = spice_dir.join("de440s.bsp");

        let optional_text_pck = spice_dir.join("pck00011.tpc");
        let optional_gravity = spice_dir.join("gm_de440.tpc");

        if !leap_seconds.is_file() || !planetary_ephemeris.is_file() {
            return Self {
                state: EphemerisState::Fallback,
                status_line: format!(
                    "Fallback orbit mode active: missing kernels. Expected {} and {}",
                    leap_seconds.display(),
                    planetary_ephemeris.display()
                ),
            };
        }

        let lock = match SpiceLock::try_acquire() {
            Ok(lock) => lock,
            Err(err) => {
                return Self {
                    state: EphemerisState::Fallback,
                    status_line: format!(
                        "Fallback orbit mode active: could not acquire SPICE lock ({err})"
                    ),
                };
            }
        };

        lock.furnsh(&leap_seconds.to_string_lossy());
        lock.furnsh(&planetary_ephemeris.to_string_lossy());

        if optional_text_pck.is_file() {
            lock.furnsh(&optional_text_pck.to_string_lossy());
        }

        if optional_gravity.is_file() {
            lock.furnsh(&optional_gravity.to_string_lossy());
        }

        let utc_now = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let base_et = lock.str2et(&spice_utc_timestamp_input(&utc_now));

        let status_line = format!(
            "SPICE mode active: loaded {}, {}{}{}",
            leap_seconds.display(),
            planetary_ephemeris.display(),
            if optional_text_pck.is_file() {
                format!(", {}", optional_text_pck.display())
            } else {
                String::new()
            },
            if optional_gravity.is_file() {
                format!(", {}", optional_gravity.display())
            } else {
                String::new()
            }
        );

        Self {
            state: EphemerisState::Spice {
                lock: Mutex::new(lock),
                base_et,
            },
            status_line,
        }
    }

    pub fn status_line(&self) -> &str {
        &self.status_line
    }

    pub fn is_spice_enabled(&self) -> bool {
        #[cfg(feature = "spice")]
        {
            matches!(self.state, EphemerisState::Spice { .. })
        }

        #[cfg(not(feature = "spice"))]
        {
            false
        }
    }

    #[cfg(feature = "spice")]
    pub fn position_au(&self, target: &str, elapsed_simulation_days: f64) -> [f64; 3] {
        if target.eq_ignore_ascii_case("SUN") {
            return [0.0, 0.0, 0.0];
        }
        if !spice_supports_target(target) {
            return fallback_position_au(target, elapsed_simulation_days);
        }

        match &self.state {
            EphemerisState::Spice { lock, base_et } => {
                let et = *base_et + elapsed_simulation_days * SECONDS_PER_DAY;
                let sl = lock.lock().expect("SPICE lock poisoned");
                spice_position_au_at_et(&sl, target, et)
            }
            EphemerisState::Fallback => fallback_position_au(target, elapsed_simulation_days),
        }
    }

    #[cfg(not(feature = "spice"))]
    pub fn position_au(&self, target: &str, elapsed_simulation_days: f64) -> [f64; 3] {
        if target.eq_ignore_ascii_case("SUN") {
            [0.0, 0.0, 0.0]
        } else {
            fallback_position_au(target, elapsed_simulation_days)
        }
    }

    #[cfg(feature = "spice")]
    pub fn position_au_at_utc_timestamp(&self, target: &str, utc_timestamp: &str) -> [f64; 3] {
        if target.eq_ignore_ascii_case("SUN") {
            return [0.0, 0.0, 0.0];
        }

        match &self.state {
            EphemerisState::Spice { lock, base_et } => {
                let sl = lock.lock().expect("SPICE lock poisoned");
                let et = sl.str2et(&spice_utc_timestamp_input(utc_timestamp));

                if spice_supports_target(target) {
                    spice_position_au_at_et(&sl, target, et)
                } else {
                    let elapsed_simulation_days = (et - *base_et) / SECONDS_PER_DAY;
                    fallback_position_au(target, elapsed_simulation_days)
                }
            }
            EphemerisState::Fallback => fallback_position_au(target, 0.0),
        }
    }

    #[cfg(not(feature = "spice"))]
    pub fn position_au_at_utc_timestamp(&self, target: &str, utc_timestamp: &str) -> [f64; 3] {
        let _ = utc_timestamp;
        self.position_au(target, 0.0)
    }
}

#[cfg(feature = "spice")]
fn spice_position_au_at_et(lock: &SpiceLock, target: &str, et: f64) -> [f64; 3] {
    let (position_km, _light_time) = lock.spkpos(target, et, SPICE_REFERENCE_FRAME, "NONE", "SUN");

    [
        position_km[0] / KM_PER_AU,
        position_km[1] / KM_PER_AU,
        position_km[2] / KM_PER_AU,
    ]
}

#[cfg(feature = "spice")]
fn spice_utc_timestamp_input(utc_timestamp: &str) -> String {
    if utc_timestamp.ends_with('Z') || utc_timestamp.to_ascii_uppercase().contains("UTC") {
        utc_timestamp.to_string()
    } else {
        format!("{utc_timestamp} UTC")
    }
}

impl Drop for SpiceEphemeris {
    fn drop(&mut self) {
        #[cfg(feature = "spice")]
        {
            if let EphemerisState::Spice { lock, .. } = &self.state
                && let Ok(sl) = lock.lock()
            {
                sl.kclear();
            }
        }
    }
}

pub fn horizons_command_for_target(target: &str) -> Option<&'static str> {
    match target {
        "SUN" => Some("10"),
        "MERCURY BARYCENTER" => Some("1"),
        "VENUS BARYCENTER" => Some("2"),
        "EARTH BARYCENTER" => Some("3"),
        "MOON" => Some("301"),
        "MARS BARYCENTER" => Some("4"),
        "JUPITER BARYCENTER" => Some("5"),
        "SATURN BARYCENTER" => Some("6"),
        "URANUS BARYCENTER" => Some("7"),
        "NEPTUNE BARYCENTER" => Some("8"),
        "CERES" => Some("1;"),
        "VESTA" => Some("4;"),
        "PLUTO BARYCENTER" => Some("9"),
        "PLUTO" => Some("999"),
        "CHARON" => Some("901"),
        _ => None,
    }
}

pub fn build_horizons_client(timeout: Duration) -> Result<Client, String> {
    Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|err| format!("could not build HTTP client: {err}"))
}

pub fn fetch_horizons_heliocentric_position_au_with_client(
    client: &Client,
    command: &str,
    utc_time: &str,
) -> Result<[f64; 3], String> {
    let command_value = format!("'{command}'");
    let tlist_value = format!("'{utc_time}'");

    let response = client
        .get(HORIZONS_API_URL)
        .query(&[
            ("format", "text"),
            ("MAKE_EPHEM", "YES"),
            ("OBJ_DATA", "NO"),
            ("EPHEM_TYPE", "VECTORS"),
            ("COMMAND", command_value.as_str()),
            ("CENTER", HORIZONS_CENTER),
            ("TLIST", tlist_value.as_str()),
            ("TIME_TYPE", "UT"),
            ("REF_SYSTEM", "ICRF"),
            ("REF_PLANE", "ECLIPTIC"),
            ("OUT_UNITS", "AU-D"),
            ("VEC_CORR", "NONE"),
            ("VEC_TABLE", "1"),
            ("CSV_FORMAT", "YES"),
            ("VEC_LABELS", "NO"),
        ])
        .send()
        .map_err(|err| format!("request failed: {err}"))?
        .error_for_status()
        .map_err(|err| format!("request failed: {err}"))?;

    let text = response
        .text()
        .map_err(|err| format!("could not read response: {err}"))?;

    parse_horizons_vector_row_au(&text)
}

fn parse_horizons_vector_row_au(raw: &str) -> Result<[f64; 3], String> {
    let mut in_data = false;

    for line in raw.lines() {
        let trimmed = line.trim();

        if trimmed == "$$SOE" {
            in_data = true;
            continue;
        }

        if trimmed == "$$EOE" {
            break;
        }

        if !in_data || trimmed.is_empty() {
            continue;
        }

        let fields: Vec<&str> = trimmed
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect();

        if fields.len() < 5 {
            return Err(format!("unexpected data row: {trimmed}"));
        }

        let parse_value = |index: usize| -> Result<f64, String> {
            fields[index]
                .replace('D', "E")
                .parse::<f64>()
                .map_err(|err| format!("invalid numeric field `{}`: {err}", fields[index]))
        };

        let x = parse_value(2)?;
        let y = parse_value(3)?;
        let z = parse_value(4)?;
        return Ok([x, y, z]);
    }

    let first_line = raw.lines().next().unwrap_or("no response body");
    Err(format!("missing Horizons vector data block ({first_line})"))
}

fn fallback_position_au(target: &str, elapsed_days: f64) -> [f64; 3] {
    if target.eq_ignore_ascii_case("MOON") {
        let earth = fallback_planet_position_au("EARTH", elapsed_days);
        let moon_radius_au = MOON_SEMI_MAJOR_AXIS_KM / KM_PER_AU;
        let theta = std::f64::consts::TAU * elapsed_days / 27.321661 + 0.35;

        let x = earth[0] + moon_radius_au * theta.cos();
        let y = earth[1] + moon_radius_au * theta.sin();
        let z = earth[2] + moon_radius_au * 0.12 * (theta * 0.5).sin();

        return [x, y, z];
    }
    if target.eq_ignore_ascii_case("CHARON") {
        let pluto = fallback_planet_position_au("PLUTO BARYCENTER", elapsed_days);
        let charon_radius_au = CHARON_SEMI_MAJOR_AXIS_KM / KM_PER_AU;
        let theta = std::f64::consts::TAU * elapsed_days / 6.38723 + 1.1;

        let x = pluto[0] + charon_radius_au * theta.cos();
        let y = pluto[1] + charon_radius_au * theta.sin();
        let z = pluto[2] + charon_radius_au * 0.03 * (theta * 1.4).sin();

        return [x, y, z];
    }

    fallback_planet_position_au(target, elapsed_days)
}

fn fallback_planet_position_au(target: &str, elapsed_days: f64) -> [f64; 3] {
    let Some(orbit) = orbit_for_target(target) else {
        return [0.0, 0.0, 0.0];
    };

    let theta = orbit.phase_radians + std::f64::consts::TAU * elapsed_days / orbit.period_days;
    let x = orbit.semi_major_axis_au * theta.cos();
    let y = orbit.semi_major_axis_au * theta.sin();
    let z = y * orbit.inclination_radians.sin();

    [x, y, z]
}

fn orbit_for_target(target: &str) -> Option<FallbackOrbit> {
    match target {
        "MERCURY" | "MERCURY BARYCENTER" => Some(FallbackOrbit {
            semi_major_axis_au: 0.387,
            period_days: 87.969,
            phase_radians: 1.0,
            inclination_radians: 7.0_f64.to_radians(),
        }),
        "VENUS" | "VENUS BARYCENTER" => Some(FallbackOrbit {
            semi_major_axis_au: 0.723,
            period_days: 224.701,
            phase_radians: 2.3,
            inclination_radians: 3.4_f64.to_radians(),
        }),
        "EARTH" | "EARTH BARYCENTER" => Some(FallbackOrbit {
            semi_major_axis_au: 1.0,
            period_days: 365.256,
            phase_radians: 0.0,
            inclination_radians: 0.0,
        }),
        "MARS" | "MARS BARYCENTER" => Some(FallbackOrbit {
            semi_major_axis_au: 1.524,
            period_days: 686.98,
            phase_radians: 1.9,
            inclination_radians: 1.85_f64.to_radians(),
        }),
        "CERES" => Some(FallbackOrbit {
            semi_major_axis_au: 2.767,
            period_days: 1680.0,
            phase_radians: 0.38,
            inclination_radians: 10.6_f64.to_radians(),
        }),
        "VESTA" => Some(FallbackOrbit {
            semi_major_axis_au: 2.361,
            period_days: 1325.0,
            phase_radians: 1.42,
            inclination_radians: 7.1_f64.to_radians(),
        }),
        "JUPITER" | "JUPITER BARYCENTER" => Some(FallbackOrbit {
            semi_major_axis_au: 5.204,
            period_days: 4332.589,
            phase_radians: 0.7,
            inclination_radians: 1.3_f64.to_radians(),
        }),
        "SATURN" | "SATURN BARYCENTER" => Some(FallbackOrbit {
            semi_major_axis_au: 9.582,
            period_days: 10_759.22,
            phase_radians: 2.8,
            inclination_radians: 2.5_f64.to_radians(),
        }),
        "URANUS" | "URANUS BARYCENTER" => Some(FallbackOrbit {
            semi_major_axis_au: 19.201,
            period_days: 30_688.5,
            phase_radians: 4.1,
            inclination_radians: 0.77_f64.to_radians(),
        }),
        "NEPTUNE" | "NEPTUNE BARYCENTER" => Some(FallbackOrbit {
            semi_major_axis_au: 30.047,
            period_days: 60_182.0,
            phase_radians: 5.4,
            inclination_radians: 1.77_f64.to_radians(),
        }),
        "PLUTO" | "PLUTO BARYCENTER" => Some(FallbackOrbit {
            semi_major_axis_au: 39.482,
            period_days: 90_560.0,
            phase_radians: 3.74,
            inclination_radians: 17.16_f64.to_radians(),
        }),
        _ => None,
    }
}

#[cfg(feature = "spice")]
fn spice_supports_target(target: &str) -> bool {
    matches!(
        target,
        "SUN"
            | "MERCURY BARYCENTER"
            | "VENUS BARYCENTER"
            | "EARTH BARYCENTER"
            | "MOON"
            | "MARS BARYCENTER"
            | "JUPITER BARYCENTER"
            | "SATURN BARYCENTER"
            | "URANUS BARYCENTER"
            | "NEPTUNE BARYCENTER"
            | "PLUTO BARYCENTER"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    fn assert_close(actual: f64, expected: f64, epsilon: f64) {
        assert!(
            (actual - expected).abs() <= epsilon,
            "expected {expected}, got {actual} (|delta| = {})",
            (actual - expected).abs()
        );
    }

    #[test]
    fn parse_horizons_vector_row_au_parses_nominal_csv_row() {
        let raw = "\
*******************************************************************************
$$SOE
2460400.500000000, A.D. 2024-Apr-01 00:00:00.0000, 1.0, -2.5, 3.25,
$$EOE
*******************************************************************************
";

        let parsed = parse_horizons_vector_row_au(raw).expect("expected parse success");
        assert_eq!(parsed, [1.0, -2.5, 3.25]);
    }

    #[test]
    fn parse_horizons_vector_row_au_parses_fortran_d_notation() {
        let raw = "\
$$SOE
2460400.500000000, A.D. 2024-Apr-01 00:00:00.0000, 1.234D+00, -5.000D-01, 9.900D+01,
$$EOE
";

        let parsed = parse_horizons_vector_row_au(raw).expect("expected parse success");
        assert_close(parsed[0], 1.234, EPS);
        assert_close(parsed[1], -0.5, EPS);
        assert_close(parsed[2], 99.0, EPS);
    }

    #[test]
    fn parse_horizons_vector_row_au_errors_when_data_block_is_missing() {
        let raw = "Horizons response without SOE/EOE markers";
        let err = parse_horizons_vector_row_au(raw).expect_err("expected parse failure");
        assert!(err.contains("missing Horizons vector data block"));
    }

    #[test]
    fn parse_horizons_vector_row_au_errors_on_short_data_row() {
        let raw = "\
$$SOE
2460400.500000000, A.D. 2024-Apr-01 00:00:00.0000, 1.0
$$EOE
";
        let err = parse_horizons_vector_row_au(raw).expect_err("expected parse failure");
        assert!(err.contains("unexpected data row"));
    }

    #[test]
    fn fallback_planet_position_au_earth_is_periodic() {
        let start = fallback_planet_position_au("EARTH", 0.0);
        let one_orbit = fallback_planet_position_au("EARTH", 365.256);

        assert_close(start[0], one_orbit[0], 1e-9);
        assert_close(start[1], one_orbit[1], 1e-9);
        assert_close(start[2], one_orbit[2], 1e-9);
    }

    #[test]
    fn fallback_planet_position_au_matches_earth_reference_at_day_zero() {
        let earth = fallback_planet_position_au("EARTH", 0.0);
        assert_close(earth[0], 1.0, EPS);
        assert_close(earth[1], 0.0, EPS);
        assert_close(earth[2], 0.0, EPS);
    }

    #[test]
    fn fallback_position_au_moon_xy_radius_matches_semi_major_axis() {
        let elapsed_days = 42.0;
        let moon = fallback_position_au("MOON", elapsed_days);
        let earth = fallback_planet_position_au("EARTH", elapsed_days);

        let dx = moon[0] - earth[0];
        let dy = moon[1] - earth[1];
        let xy_radius = (dx * dx + dy * dy).sqrt();
        let expected = MOON_SEMI_MAJOR_AXIS_KM / KM_PER_AU;

        assert_close(xy_radius, expected, 1e-12);
    }

    #[test]
    fn fallback_position_au_charon_xy_radius_matches_semi_major_axis() {
        let elapsed_days = 133.7;
        let charon = fallback_position_au("CHARON", elapsed_days);
        let pluto = fallback_planet_position_au("PLUTO BARYCENTER", elapsed_days);

        let dx = charon[0] - pluto[0];
        let dy = charon[1] - pluto[1];
        let xy_radius = (dx * dx + dy * dy).sqrt();
        let expected = CHARON_SEMI_MAJOR_AXIS_KM / KM_PER_AU;

        assert_close(xy_radius, expected, 1e-12);
    }

    #[test]
    fn fallback_position_au_unknown_target_defaults_to_origin() {
        assert_eq!(
            fallback_position_au("NOT_A_REAL_TARGET", 12.34),
            [0.0, 0.0, 0.0]
        );
    }

    #[cfg(not(feature = "spice"))]
    #[test]
    fn position_au_at_utc_timestamp_matches_day_zero_without_spice() {
        let ephemeris = SpiceEphemeris::new(std::path::Path::new("."));
        let at_timestamp = ephemeris.position_au_at_utc_timestamp("EARTH", "2026-04-19 12:00:00");
        let day_zero = ephemeris.position_au("EARTH", 0.0);
        assert_eq!(at_timestamp, day_zero);
    }
}
