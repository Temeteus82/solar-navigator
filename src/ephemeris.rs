use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
#[cfg(feature = "spice")]
use spice::SpiceLock;
use std::f64::consts::TAU;
use std::path::Path;
#[cfg(feature = "spice")]
use std::sync::Mutex;
use std::time::Duration;

/// Shared with `app::types`, which re-exports both so the app and the
/// ephemeris can never disagree on them.
pub const SECONDS_PER_DAY: f64 = 86_400.0;
pub const KM_PER_AU: f64 = 149_597_870.7;
const MOON_SEMI_MAJOR_AXIS_KM: f64 = 384_400.0;
const CHARON_SEMI_MAJOR_AXIS_KM: f64 = 19_591.0;
#[cfg(feature = "spice")]
const SPICE_REFERENCE_FRAME: &str = "ECLIPJ2000";
const HORIZONS_API_URL: &str = "https://ssd.jpl.nasa.gov/api/horizons.api";
const HORIZONS_CENTER: &str = "'500@10'";

pub const VOYAGER_1_TARGET: &str = "VOYAGER 1";
// Voyager 1 is not in the planetary kernels, so it always flies this linear
// model: its heliocentric ECLIPJ2000 position at 2026-01-01 00:00 UT plus the
// mean velocity over 2026–2030, both from JPL Horizons (target -31). Long past
// its last planetary encounter (Saturn, 1980) it coasts on a near-straight
// hyperbolic escape, so the line holds to ~0.01 AU across that span and stays
// a fair sketch for the decades either side. In SPICE mode the Horizons sync
// corrects whatever residual remains at the current date.
const VOYAGER_1_EPOCH_UNIX_DAYS: f64 = 20_454.0; // 2026-01-01T00:00:00Z
const VOYAGER_1_EPOCH_POSITION_AU: [f64; 3] = [
    -31.836_414_919_295_77,
    -134.678_491_196_756_7,
    97.444_810_751_349_93,
];
const VOYAGER_1_VELOCITY_AU_PER_DAY: [f64; 3] = [
    -0.001_191_018_381_385_094,
    -0.007_859_171_278_867_077,
    0.005_675_897_292_261_714,
];

// Ceres and Vesta are in no loaded kernel (de440s carries only planets), so
// even SPICE mode propagates these two-body Keplerian orbits. Osculating
// heliocentric elements, ecliptic & equinox J2000 (= ECLIPJ2000), from the JPL
// Small-Body Database (full precision, solution 2021-04-13). Checked against
// Horizons: ~9 000 km (Ceres) / ~3 000 km (Vesta) at 2026-10-08, growing to
// ~0.015 AU by 2030 and ~0.006 AU back in 2020 from Jupiter's perturbations,
// which the Horizons sync corrects at the current date.
struct KeplerElements {
    epoch_unix_days: f64,
    semi_major_axis_au: f64,
    eccentricity: f64,
    inclination_deg: f64,
    ascending_node_deg: f64,
    arg_perihelion_deg: f64,
    mean_anomaly_at_epoch_deg: f64,
    mean_motion_deg_per_day: f64,
}

const SBDB_ELEMENTS_EPOCH_UNIX_DAYS: f64 = 20_613.0; // JD 2461200.5 TDB, 2026-06-09

const CERES_ELEMENTS: KeplerElements = KeplerElements {
    epoch_unix_days: SBDB_ELEMENTS_EPOCH_UNIX_DAYS,
    semi_major_axis_au: 2.765_552_595_034_094,
    eccentricity: 0.079_692_295_148_165_86,
    inclination_deg: 10.588_027_801_834_62,
    ascending_node_deg: 80.248_626_820_432_21,
    arg_perihelion_deg: 73.294_214_530_215_87,
    mean_anomaly_at_epoch_deg: 274.419_346_376_134_2,
    mean_motion_deg_per_day: 0.214_304_450_648_43,
};

const VESTA_ELEMENTS: KeplerElements = KeplerElements {
    epoch_unix_days: SBDB_ELEMENTS_EPOCH_UNIX_DAYS,
    semi_major_axis_au: 2.361_365_965_127_599,
    eccentricity: 0.090_203_743_828_343_95,
    inclination_deg: 7.143_925_545_058_711,
    ascending_node_deg: 103.701_293_265_032,
    arg_perihelion_deg: 151.468_647_822_156_4,
    mean_anomaly_at_epoch_deg: 81.190_156_076_869_03,
    mean_motion_deg_per_day: 0.271_618_361_359_990_9,
};

// Pluto system barycentre (SBDB 134340), epoch JD 2457588.5 TDB (2016-07-19).
// Pluto is absent from the planet tables below, and SPICE mode reads it from
// de440s, so this only drives portable mode. Against Horizons: ~0.04 AU today,
// but Neptune's perturbations grow it to ~1-1.6 AU (~2 deg) by 1600 and 2200.
const PLUTO_ELEMENTS: KeplerElements = KeplerElements {
    epoch_unix_days: 17_001.0,
    semi_major_axis_au: 39.588_629_385_171_24,
    eccentricity: 0.251_837_877_857_689_2,
    inclination_deg: 17.147_711_409_991_14,
    ascending_node_deg: 110.292_384_054_305_7,
    arg_perihelion_deg: 113.709_001_515_856_5,
    mean_anomaly_at_epoch_deg: 38.683_663_473_181_84,
    mean_motion_deg_per_day: 0.003_956_838_955_553_025,
};

fn minor_body_elements(target: &str) -> Option<&'static KeplerElements> {
    match target {
        "CERES" => Some(&CERES_ELEMENTS),
        "VESTA" => Some(&VESTA_ELEMENTS),
        "PLUTO" | "PLUTO BARYCENTER" => Some(&PLUTO_ELEMENTS),
        _ => None,
    }
}

/// Mean orbital elements as `[value at J2000, rate per Julian century]`, from
/// JPL's "Approximate Positions of the Planets" (E. M. Standish), Table 2a:
/// mean ecliptic and equinox of J2000, valid 3000 BC - 3000 AD.
/// <https://ssd.jpl.nasa.gov/planets/approx_pos.html>
struct PlanetMeanElements {
    semi_major_axis_au: [f64; 2],
    eccentricity: [f64; 2],
    inclination_deg: [f64; 2],
    mean_longitude_deg: [f64; 2],
    long_perihelion_deg: [f64; 2],
    ascending_node_deg: [f64; 2],
    /// Table 2b's `[b, c, s, f]` mean-anomaly correction for the outer planets.
    mean_anomaly_terms: Option<[f64; 4]>,
}

// Checked against Horizons over the app's 1600-2200 date range: Mercury, Venus
// and the Earth-Moon barycentre within ~0.01 deg, Mars 0.03 deg, the giants 0.3 deg.
const MERCURY_ELEMENTS: PlanetMeanElements = PlanetMeanElements {
    semi_major_axis_au: [0.387_098_43, 0.0],
    eccentricity: [0.205_636_61, 0.000_021_23],
    inclination_deg: [7.005_594_32, -0.005_901_58],
    mean_longitude_deg: [252.251_667_24, 149_472.674_866_23],
    long_perihelion_deg: [77.457_718_95, 0.159_400_13],
    ascending_node_deg: [48.339_618_19, -0.122_141_82],
    mean_anomaly_terms: None,
};
const VENUS_ELEMENTS: PlanetMeanElements = PlanetMeanElements {
    semi_major_axis_au: [0.723_321_02, -0.000_000_26],
    eccentricity: [0.006_763_99, -0.000_051_07],
    inclination_deg: [3.397_775_45, 0.000_434_94],
    mean_longitude_deg: [181.979_708_50, 58_517.815_602_60],
    long_perihelion_deg: [131.767_557_13, 0.056_796_48],
    ascending_node_deg: [76.672_614_96, -0.272_741_74],
    mean_anomaly_terms: None,
};
/// Earth-Moon barycentre: within ~4 700 km of Earth itself.
const EARTH_MOON_BARYCENTER_ELEMENTS: PlanetMeanElements = PlanetMeanElements {
    semi_major_axis_au: [1.000_000_18, -0.000_000_03],
    eccentricity: [0.016_731_63, -0.000_036_61],
    inclination_deg: [-0.000_543_46, -0.013_371_78],
    mean_longitude_deg: [100.466_915_72, 35_999.373_063_29],
    long_perihelion_deg: [102.930_058_85, 0.317_952_60],
    ascending_node_deg: [-5.112_603_89, -0.241_238_56],
    mean_anomaly_terms: None,
};
const MARS_ELEMENTS: PlanetMeanElements = PlanetMeanElements {
    semi_major_axis_au: [1.523_712_43, 0.000_000_97],
    eccentricity: [0.093_365_11, 0.000_091_49],
    inclination_deg: [1.851_818_69, -0.007_247_57],
    mean_longitude_deg: [-4.568_131_64, 19_140.299_342_43],
    long_perihelion_deg: [-23.917_447_84, 0.452_236_25],
    ascending_node_deg: [49.713_209_84, -0.268_524_31],
    mean_anomaly_terms: None,
};
const JUPITER_ELEMENTS: PlanetMeanElements = PlanetMeanElements {
    semi_major_axis_au: [5.202_480_19, -0.000_028_64],
    eccentricity: [0.048_535_90, 0.000_180_26],
    inclination_deg: [1.298_614_16, -0.003_226_99],
    mean_longitude_deg: [34.334_791_52, 3_034.903_717_57],
    long_perihelion_deg: [14.274_952_44, 0.181_991_96],
    ascending_node_deg: [100.292_826_54, 0.130_246_19],
    mean_anomaly_terms: Some([-0.000_124_52, 0.060_640_60, -0.356_354_38, 38.351_250_00]),
};
const SATURN_ELEMENTS: PlanetMeanElements = PlanetMeanElements {
    semi_major_axis_au: [9.541_498_83, -0.000_030_65],
    eccentricity: [0.055_508_25, -0.000_320_44],
    inclination_deg: [2.494_241_02, 0.004_519_69],
    mean_longitude_deg: [50.075_713_29, 1_222.114_947_24],
    long_perihelion_deg: [92.861_360_63, 0.541_794_78],
    ascending_node_deg: [113.639_987_02, -0.250_150_02],
    mean_anomaly_terms: Some([0.000_258_99, -0.134_344_69, 0.873_201_47, 38.351_250_00]),
};
const URANUS_ELEMENTS: PlanetMeanElements = PlanetMeanElements {
    semi_major_axis_au: [19.187_979_48, -0.000_204_55],
    eccentricity: [0.046_857_40, -0.000_015_50],
    inclination_deg: [0.772_981_27, -0.001_801_55],
    mean_longitude_deg: [314.202_766_25, 428.495_125_95],
    long_perihelion_deg: [172.434_044_41, 0.092_669_85],
    ascending_node_deg: [73.962_502_15, 0.057_396_99],
    mean_anomaly_terms: Some([0.000_583_31, -0.977_318_48, 0.176_892_45, 7.670_250_00]),
};
const NEPTUNE_ELEMENTS: PlanetMeanElements = PlanetMeanElements {
    semi_major_axis_au: [30.069_527_52, 0.000_064_47],
    eccentricity: [0.008_954_39, 0.000_008_18],
    inclination_deg: [1.770_055_20, 0.000_224_00],
    mean_longitude_deg: [304.222_892_87, 218.465_153_14],
    long_perihelion_deg: [46.681_587_24, 0.010_099_38],
    ascending_node_deg: [131.786_358_53, -0.006_063_02],
    mean_anomaly_terms: Some([-0.000_413_48, 0.683_463_18, -0.101_625_47, 7.670_250_00]),
};

fn planet_mean_elements(target: &str) -> Option<&'static PlanetMeanElements> {
    match target {
        "MERCURY" | "MERCURY BARYCENTER" => Some(&MERCURY_ELEMENTS),
        "VENUS" | "VENUS BARYCENTER" => Some(&VENUS_ELEMENTS),
        "EARTH" | "EARTH BARYCENTER" => Some(&EARTH_MOON_BARYCENTER_ELEMENTS),
        "MARS" | "MARS BARYCENTER" => Some(&MARS_ELEMENTS),
        "JUPITER" | "JUPITER BARYCENTER" => Some(&JUPITER_ELEMENTS),
        "SATURN" | "SATURN BARYCENTER" => Some(&SATURN_ELEMENTS),
        "URANUS" | "URANUS BARYCENTER" => Some(&URANUS_ELEMENTS),
        "NEPTUNE" | "NEPTUNE BARYCENTER" => Some(&NEPTUNE_ELEMENTS),
        _ => None,
    }
}

/// J2000.0 (JD 2451545.0), in days since the Unix epoch.
const J2000_UNIX_DAYS: f64 = 10_957.5;
const DAYS_PER_JULIAN_CENTURY: f64 = 36_525.0;

/// A heliocentric Keplerian ellipse (ECLIPJ2000, angles in radians) with the
/// body's mean anomaly on it at one instant. `position_au` is where the body
/// is; `position_at_mean_anomaly_au` traces the whole ellipse for an orbit
/// ring that passes through that point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OsculatingOrbit {
    pub semi_major_axis_au: f64,
    pub eccentricity: f64,
    pub inclination: f64,
    pub ascending_node: f64,
    pub arg_perihelion: f64,
    pub mean_anomaly: f64,
}

impl OsculatingOrbit {
    pub fn position_au(&self) -> [f64; 3] {
        self.position_at_mean_anomaly_au(self.mean_anomaly)
    }

    pub fn position_at_mean_anomaly_au(&self, mean_anomaly: f64) -> [f64; 3] {
        orbital_position_au(
            self.semi_major_axis_au,
            self.eccentricity,
            self.inclination,
            self.ascending_node,
            self.arg_perihelion,
            mean_anomaly,
        )
    }
}

/// The approximate-positions recipe: evaluate each element at `T` Julian
/// centuries past J2000. UTC stands in for TDB here; the ~70 s difference is
/// far below the tables' own accuracy.
fn planet_orbit(elements: &PlanetMeanElements, unix_days: f64) -> OsculatingOrbit {
    let t = (unix_days - J2000_UNIX_DAYS) / DAYS_PER_JULIAN_CENTURY;
    let at = |[value, rate]: [f64; 2]| value + rate * t;

    let long_perihelion = at(elements.long_perihelion_deg);
    let ascending_node = at(elements.ascending_node_deg);
    let mut mean_anomaly = at(elements.mean_longitude_deg) - long_perihelion;
    if let Some([b, c, s, f]) = elements.mean_anomaly_terms {
        let (sin_ft, cos_ft) = (f * t).to_radians().sin_cos();
        mean_anomaly += b * t * t + c * cos_ft + s * sin_ft;
    }

    OsculatingOrbit {
        semi_major_axis_au: at(elements.semi_major_axis_au),
        eccentricity: at(elements.eccentricity),
        inclination: at(elements.inclination_deg).to_radians(),
        ascending_node: ascending_node.to_radians(),
        arg_perihelion: (long_perihelion - ascending_node).to_radians(),
        mean_anomaly: mean_anomaly.to_radians(),
    }
}

fn minor_body_orbit(elements: &KeplerElements, unix_days: f64) -> OsculatingOrbit {
    OsculatingOrbit {
        semi_major_axis_au: elements.semi_major_axis_au,
        eccentricity: elements.eccentricity,
        inclination: elements.inclination_deg.to_radians(),
        ascending_node: elements.ascending_node_deg.to_radians(),
        arg_perihelion: elements.arg_perihelion_deg.to_radians(),
        mean_anomaly: (elements.mean_anomaly_at_epoch_deg
            + elements.mean_motion_deg_per_day * (unix_days - elements.epoch_unix_days))
            .to_radians(),
    }
}

/// The heliocentric ellipse `target` rides on `unix_days`: JPL mean elements
/// for the planets, SBDB osculating elements for Ceres, Vesta and Pluto.
/// `None` for the Sun, satellites, Voyager and unknown targets. In SPICE mode
/// this is the analytic stand-in the orbit ring is drawn from; it stays within
/// the tables' ~0.3 deg of the kernel positions.
pub fn heliocentric_orbit(target: &str, unix_days: f64) -> Option<OsculatingOrbit> {
    if let Some(elements) = minor_body_elements(target) {
        Some(minor_body_orbit(elements, unix_days))
    } else {
        planet_mean_elements(target).map(|elements| planet_orbit(elements, unix_days))
    }
}

/// Heliocentric ECLIPJ2000 position (AU) on a Keplerian ellipse with the given
/// elements (angles in radians) at `mean_anomaly`.
#[inline]
pub fn orbital_position_au(
    semi_major_axis: f64,
    eccentricity: f64,
    inclination: f64,
    ascending_node: f64,
    arg_perihelion: f64,
    mean_anomaly: f64,
) -> [f64; 3] {
    let eccentric_anomaly = solve_kepler(mean_anomaly, eccentricity);
    let (sin_e, cos_e) = eccentric_anomaly.sin_cos();

    // Position in the orbital plane (perifocal frame).
    let x_perifocal = semi_major_axis * (cos_e - eccentricity);
    let y_perifocal = semi_major_axis * (1.0 - eccentricity * eccentricity).sqrt() * sin_e;

    // Rotate perifocal → ECLIPJ2000 via (ω, i, Ω) Euler angles.
    let (sin_w, cos_w) = arg_perihelion.sin_cos();
    let (sin_i, cos_i) = inclination.sin_cos();
    let (sin_o, cos_o) = ascending_node.sin_cos();

    [
        (cos_o * cos_w - sin_o * sin_w * cos_i) * x_perifocal
            + (-cos_o * sin_w - sin_o * cos_w * cos_i) * y_perifocal,
        (sin_o * cos_w + cos_o * sin_w * cos_i) * x_perifocal
            + (-sin_o * sin_w + cos_o * cos_w * cos_i) * y_perifocal,
        (sin_w * sin_i) * x_perifocal + (cos_w * sin_i) * y_perifocal,
    ]
}

/// Newton's-method solve of Kepler's equation `M = E - e·sin(E)`.
/// Five iterations from `E₀ = M` converge to ~1e-15 rad for every
/// eccentricity routed through here (Pluto's 0.252 is the highest; the test
/// covers 0.26). Orbits with `e ≥ 0.8` start from `E₀ = π` instead.
#[inline]
fn solve_kepler(mean_anomaly: f64, eccentricity: f64) -> f64 {
    let m = mean_anomaly.rem_euclid(TAU);
    let mut e_anom = if eccentricity < 0.8 {
        m
    } else {
        std::f64::consts::PI
    };
    for _ in 0..5 {
        let f = e_anom - eccentricity * e_anom.sin() - m;
        let f_prime = 1.0 - eccentricity * e_anom.cos();
        e_anom -= f / f_prime;
    }
    e_anom
}

#[cfg(feature = "spice")]
enum EphemerisState {
    #[cfg(feature = "spice")]
    Spice {
        lock: Mutex<SpiceLock>,
        base_et: f64,
        /// `[first, last]` ephemeris time the planetary kernel covers, from
        /// `spkcov`. Queries outside it use the analytic orbits instead of
        /// asking CSPICE, which would only signal `SPICE(SPKINSUFFDATA)`.
        coverage_et: Option<(f64, f64)>,
    },
    Fallback,
}

/// Whether `et` lies inside the kernel's coverage window. `None` means the
/// window could not be read, in which case every query is attempted and the
/// error check after it decides.
#[cfg(feature = "spice")]
fn et_within_coverage(coverage_et: Option<(f64, f64)>, et: f64) -> bool {
    coverage_et.is_none_or(|(first, last)| (first..=last).contains(&et))
}

pub struct SpiceEphemeris {
    #[cfg(feature = "spice")]
    state: EphemerisState,
    status_line: String,
    /// Wall-clock time at construction: simulation day 0. The one launch
    /// instant — the SPICE epoch, every analytic position and the app's date
    /// label (`SimulationEpoch`) derive from it.
    start_utc: DateTime<Utc>,
    /// `start_utc` in days since the Unix epoch, the unit the analytic
    /// orbits are propagated in.
    start_unix_days: f64,
}

impl SpiceEphemeris {
    #[cfg(feature = "spice")]
    pub fn new(spice_dir: &Path) -> Self {
        Self::new_with_spice(spice_dir)
    }

    #[cfg(not(feature = "spice"))]
    pub fn new(spice_dir: &Path) -> Self {
        let _ = spice_dir;
        let start_utc = Utc::now();
        Self {
            status_line: "Fallback orbit mode active: app was compiled without the `spice` feature"
                .to_string(),
            start_utc,
            start_unix_days: unix_days_of(start_utc),
        }
    }

    #[cfg(feature = "spice")]
    fn new_with_spice(spice_dir: &Path) -> Self {
        let start_utc = Utc::now();
        let start_unix_days = unix_days_of(start_utc);
        let leap_seconds = spice_dir.join("naif0012.tls");
        let planetary_ephemeris = spice_dir.join("de440s.bsp");

        let optional_text_pck = spice_dir.join("pck00011.tpc");
        let optional_gravity = spice_dir.join("gm_de440.tpc");

        let fallback = |reason: String| Self {
            state: EphemerisState::Fallback,
            status_line: format!("Fallback orbit mode active: {reason}"),
            start_utc,
            start_unix_days,
        };

        if !leap_seconds.is_file() || !planetary_ephemeris.is_file() {
            return fallback(format!(
                "missing kernels. Expected {} and {}",
                leap_seconds.display(),
                planetary_ephemeris.display()
            ));
        }

        let lock = match SpiceLock::try_acquire() {
            Ok(lock) => lock,
            Err(err) => return fallback(format!("could not acquire SPICE lock ({err})")),
        };

        // CSPICE's default error action is ABORT: the first failed call (an
        // unreadable kernel, a date outside `de440s`'s 1849-2150 coverage)
        // prints a traceback and exits the process. RETURN instead, and
        // `check()` after every call so a failure falls back to the analytic
        // orbits.
        lock.quiet();

        let mut loaded_kernels = Vec::new();
        for kernel in [&leap_seconds, &planetary_ephemeris] {
            lock.furnsh(&kernel.to_string_lossy());
            if let Err(err) = lock.check() {
                lock.kclear();
                return fallback(format!("could not load {} ({err})", kernel.display()));
            }
            loaded_kernels.push(kernel.display().to_string());
        }

        for kernel in [&optional_text_pck, &optional_gravity] {
            if !kernel.is_file() {
                continue;
            }
            lock.furnsh(&kernel.to_string_lossy());
            match lock.check() {
                Ok(()) => loaded_kernels.push(kernel.display().to_string()),
                Err(err) => eprintln!("Skipping optional kernel {} ({err})", kernel.display()),
            }
        }

        // The SPICE epoch is the same instant as `start_unix_days`, to the
        // millisecond, so kernel and analytic bodies share one clock.
        let launch = start_utc.format("%Y-%m-%d %H:%M:%S.%3f").to_string();
        let base_et = lock.str2et(&spice_utc_timestamp_input(&launch));
        if let Err(err) = lock.check() {
            lock.kclear();
            return fallback(format!("could not convert the launch time ({err})"));
        }

        let coverage_et = spk_coverage_et(&lock, &planetary_ephemeris.to_string_lossy());
        let coverage_note = match coverage_et {
            Some((first, last)) => {
                let to_date = |et: f64| {
                    let unix_days = start_unix_days + (et - base_et) / SECONDS_PER_DAY;
                    chrono::DateTime::from_timestamp((unix_days * SECONDS_PER_DAY) as i64, 0)
                        .map(|date| date.format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|| format!("ET {et:.0}"))
                };
                format!(
                    " | kernel coverage {} to {}, analytic orbits outside it",
                    to_date(first),
                    to_date(last)
                )
            }
            None => " | kernel coverage unknown".to_string(),
        };
        let status_line = format!(
            "SPICE mode active: loaded {}{coverage_note}",
            loaded_kernels.join(", ")
        );

        Self {
            state: EphemerisState::Spice {
                lock: Mutex::new(lock),
                base_et,
                coverage_et,
            },
            status_line,
            start_utc,
            start_unix_days,
        }
    }

    /// Wall-clock time at construction, simulation day 0: the app's single
    /// launch instant (see `start_utc` on the struct).
    pub fn start_utc(&self) -> DateTime<Utc> {
        self.start_utc
    }

    /// The span of dates the loaded planetary kernel covers, in days since
    /// the Unix epoch. `None` in portable mode, in fallback mode, or when the
    /// coverage could not be read.
    pub fn spice_coverage_unix_days(&self) -> Option<(f64, f64)> {
        #[cfg(feature = "spice")]
        {
            let EphemerisState::Spice {
                base_et,
                coverage_et,
                ..
            } = &self.state
            else {
                return None;
            };
            let to_unix_days = |et: f64| self.start_unix_days + (et - base_et) / SECONDS_PER_DAY;
            coverage_et.map(|(first, last)| (to_unix_days(first), to_unix_days(last)))
        }

        #[cfg(not(feature = "spice"))]
        {
            None
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
            return self.analytic_position_au(target, elapsed_simulation_days);
        }

        match &self.state {
            EphemerisState::Spice {
                lock,
                base_et,
                coverage_et,
            } => {
                let et = *base_et + elapsed_simulation_days * SECONDS_PER_DAY;
                let sl = lock.lock().expect("SPICE lock poisoned");
                spice_position_au_at_et(&sl, target, et, *coverage_et)
                    .unwrap_or_else(|| self.analytic_position_au(target, elapsed_simulation_days))
            }
            EphemerisState::Fallback => self.analytic_position_au(target, elapsed_simulation_days),
        }
    }

    #[cfg(not(feature = "spice"))]
    pub fn position_au(&self, target: &str, elapsed_simulation_days: f64) -> [f64; 3] {
        if target.eq_ignore_ascii_case("SUN") {
            [0.0, 0.0, 0.0]
        } else {
            self.analytic_position_au(target, elapsed_simulation_days)
        }
    }

    #[cfg(feature = "spice")]
    pub fn position_au_at_utc_timestamp(&self, target: &str, utc_timestamp: &str) -> [f64; 3] {
        if target.eq_ignore_ascii_case("SUN") {
            return [0.0, 0.0, 0.0];
        }

        match &self.state {
            EphemerisState::Spice {
                lock,
                base_et,
                coverage_et,
            } => {
                let sl = lock.lock().expect("SPICE lock poisoned");
                let et = sl.str2et(&spice_utc_timestamp_input(utc_timestamp));
                if let Err(err) = sl.check() {
                    eprintln!("SPICE could not parse `{utc_timestamp}` ({err}); using day zero");
                    return self.analytic_position_au(target, 0.0);
                }
                let elapsed_simulation_days = (et - *base_et) / SECONDS_PER_DAY;

                if spice_supports_target(target) {
                    spice_position_au_at_et(&sl, target, et, *coverage_et).unwrap_or_else(|| {
                        self.analytic_position_au(target, elapsed_simulation_days)
                    })
                } else {
                    self.analytic_position_au(target, elapsed_simulation_days)
                }
            }
            EphemerisState::Fallback => self.analytic_position_au(target, 0.0),
        }
    }

    #[cfg(not(feature = "spice"))]
    pub fn position_au_at_utc_timestamp(&self, target: &str, utc_timestamp: &str) -> [f64; 3] {
        let _ = utc_timestamp;
        self.position_au(target, 0.0)
    }

    /// The absolute date, in days since the Unix epoch, that
    /// `elapsed_simulation_days` from launch corresponds to. Every analytic
    /// position, satellite phase and orbit ring is evaluated on this clock.
    pub fn unix_days_at(&self, elapsed_simulation_days: f64) -> f64 {
        self.start_unix_days + elapsed_simulation_days
    }

    /// Non-SPICE position, on absolute dates: Voyager's trajectory, else the
    /// analytic orbits (`fallback_position_au`).
    fn analytic_position_au(&self, target: &str, elapsed_simulation_days: f64) -> [f64; 3] {
        let unix_days = self.unix_days_at(elapsed_simulation_days);
        if target == VOYAGER_1_TARGET {
            voyager_1_position_au(unix_days)
        } else {
            fallback_position_au(target, unix_days)
        }
    }
}

fn unix_days_of(utc: DateTime<Utc>) -> f64 {
    utc.timestamp_millis() as f64 / (1_000.0 * SECONDS_PER_DAY)
}

fn voyager_1_position_au(unix_days: f64) -> [f64; 3] {
    let dt = unix_days - VOYAGER_1_EPOCH_UNIX_DAYS;
    std::array::from_fn(|axis| {
        VOYAGER_1_EPOCH_POSITION_AU[axis] + VOYAGER_1_VELOCITY_AU_PER_DAY[axis] * dt
    })
}

/// Heliocentric position from the loaded kernels, or `None` when `et` is
/// outside their coverage or CSPICE signals an error — the caller then falls
/// back to the analytic orbit. Errors are logged once, not once per frame.
#[cfg(feature = "spice")]
fn spice_position_au_at_et(
    lock: &SpiceLock,
    target: &str,
    et: f64,
    coverage_et: Option<(f64, f64)>,
) -> Option<[f64; 3]> {
    if !et_within_coverage(coverage_et, et) {
        return None;
    }

    let (position_km, _light_time) = lock.spkpos(target, et, SPICE_REFERENCE_FRAME, "NONE", "SUN");
    if let Err(err) = lock.check() {
        static LOGGED: std::sync::Once = std::sync::Once::new();
        LOGGED.call_once(|| {
            eprintln!("SPICE query failed for {target} ({err}); using analytic orbits");
        });
        return None;
    }

    Some([
        position_km[0] / KM_PER_AU,
        position_km[1] / KM_PER_AU,
        position_km[2] / KM_PER_AU,
    ])
}

/// `[first, last]` ephemeris time over which every object in `spk` has
/// data: the intersection of the per-object `spkcov` windows (gaps inside a
/// window are ignored; the error check after each query still catches them).
#[cfg(feature = "spice")]
fn spk_coverage_et(lock: &SpiceLock, spk: &str) -> Option<(f64, f64)> {
    let ids = lock.spkobj(spk);
    if lock.check().is_err() || ids.is_empty() {
        return None;
    }

    let mut first = f64::NEG_INFINITY;
    let mut last = f64::INFINITY;
    for id in ids.iter() {
        let window = lock.spkcov(spk, id);
        if lock.check().is_err() {
            return None;
        }
        // A window is a flat list of interval endpoints: [start, end, ...].
        let start = window.get(0)?;
        let end = window.get(window.len().checked_sub(1)?)?;
        first = first.max(start);
        last = last.min(end);
    }

    (first < last).then_some((first, last))
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
        "EARTH" => Some("399"),
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
        "IO" => Some("501"),
        "EUROPA" => Some("502"),
        "GANYMEDE" => Some("503"),
        "CALLISTO" => Some("504"),
        VOYAGER_1_TARGET => Some("-31"),
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

    const MAX_HORIZONS_RESPONSE_BYTES: usize = 1_000_000;

    let bytes = response
        .bytes()
        .map_err(|err| format!("could not read response: {err}"))?;
    if bytes.len() > MAX_HORIZONS_RESPONSE_BYTES {
        return Err(format!(
            "Horizons response too large ({} bytes)",
            bytes.len()
        ));
    }
    let text = String::from_utf8_lossy(&bytes).into_owned();

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

/// Parameters describing a satellite's fallback orbit around its primary.
pub struct SatelliteOrbit {
    pub primary: &'static str,
    pub semi_major_axis_km: f64,
    pub period_days: f64,
    pub phase_radians: f64,
    pub z_wobble_factor: f64,
    pub z_wobble_frequency: f64,
}

const MOON_ORBIT: SatelliteOrbit = SatelliteOrbit {
    primary: "EARTH",
    semi_major_axis_km: MOON_SEMI_MAJOR_AXIS_KM,
    period_days: 27.321661,
    phase_radians: 0.35,
    z_wobble_factor: 0.12,
    z_wobble_frequency: 0.5,
};

pub const CHARON_ORBIT: SatelliteOrbit = SatelliteOrbit {
    primary: "PLUTO BARYCENTER",
    semi_major_axis_km: CHARON_SEMI_MAJOR_AXIS_KM,
    period_days: 6.38723,
    phase_radians: 1.1,
    // No artificial out-of-plane bob: Charon's orbit is rotated into Pluto's
    // equatorial plane by `apply_pluto_charon_center_positions`, which gives it
    // its real ~120° inclination. Adding wobble on top would just jitter both
    // bodies off the equatorial plane every frame.
    z_wobble_factor: 0.0,
    z_wobble_frequency: 0.0,
};

pub const IO_ORBIT: SatelliteOrbit = SatelliteOrbit {
    primary: "JUPITER BARYCENTER",
    semi_major_axis_km: 421_800.0,
    period_days: 1.769138,
    phase_radians: 0.4,
    z_wobble_factor: 0.04,
    z_wobble_frequency: 1.2,
};

pub const EUROPA_ORBIT: SatelliteOrbit = SatelliteOrbit {
    primary: "JUPITER BARYCENTER",
    semi_major_axis_km: 671_100.0,
    period_days: 3.551181,
    phase_radians: 2.1,
    z_wobble_factor: 0.05,
    z_wobble_frequency: 0.9,
};

pub const GANYMEDE_ORBIT: SatelliteOrbit = SatelliteOrbit {
    primary: "JUPITER BARYCENTER",
    semi_major_axis_km: 1_070_400.0,
    period_days: 7.154553,
    phase_radians: 3.8,
    z_wobble_factor: 0.04,
    z_wobble_frequency: 0.7,
};

pub const CALLISTO_ORBIT: SatelliteOrbit = SatelliteOrbit {
    primary: "JUPITER BARYCENTER",
    semi_major_axis_km: 1_882_700.0,
    period_days: 16.689018,
    phase_radians: 5.3,
    z_wobble_factor: 0.05,
    z_wobble_frequency: 0.5,
};

/// A satellite's ECLIPJ2000 offset (AU) from its primary on `unix_days`: a
/// circle in the ecliptic plane at an arbitrary but date-stable phase, plus
/// the small out-of-plane wobble. The single placement rule for every
/// analytic moon, whether the ephemeris adds it to the primary (the Moon) or
/// `simulation.rs` does after the ephemeris pass (the reconstructed ones).
pub fn satellite_offset_au(orbit: &SatelliteOrbit, unix_days: f64) -> [f64; 3] {
    let radius_au = orbit.semi_major_axis_km / KM_PER_AU;
    let theta = TAU * unix_days / orbit.period_days + orbit.phase_radians;

    [
        radius_au * theta.cos(),
        radius_au * theta.sin(),
        radius_au * orbit.z_wobble_factor * (theta * orbit.z_wobble_frequency).sin(),
    ]
}

/// Analytic heliocentric position on a real date. The Moon rides
/// `MOON_ORBIT` around the Earth-Moon barycentre; everything else is a planet
/// or minor body on its own ellipse. The reconstructed satellites (Charon and
/// the Galilean moons) are not placed here: `simulation.rs` rebuilds them
/// from their primary's scene position, so they report the origin like any
/// unknown target.
fn fallback_position_au(target: &str, unix_days: f64) -> [f64; 3] {
    if target.eq_ignore_ascii_case("MOON") {
        let earth = fallback_planet_position_au(MOON_ORBIT.primary, unix_days);
        let offset = satellite_offset_au(&MOON_ORBIT, unix_days);
        return std::array::from_fn(|axis| earth[axis] + offset[axis]);
    }

    fallback_planet_position_au(target, unix_days)
}

/// Heliocentric position of a planet or minor body on a real date: JPL mean
/// elements for the planets, osculating elements for Ceres, Vesta and Pluto.
/// Unknown targets sit at the origin.
fn fallback_planet_position_au(target: &str, unix_days: f64) -> [f64; 3] {
    heliocentric_orbit(target, unix_days).map_or([0.0, 0.0, 0.0], |orbit| orbit.position_au())
}

#[cfg(feature = "spice")]
fn spice_supports_target(target: &str) -> bool {
    matches!(
        target,
        "SUN"
            | "MERCURY BARYCENTER"
            | "VENUS BARYCENTER"
            | "EARTH"
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

    #[test]
    fn kepler_solver_converges_for_circular_orbit() {
        let e = solve_kepler(1.234, 0.0);
        // For e = 0, E should equal M (mod TAU).
        assert!((e - 1.234).abs() < 1e-9);
    }

    #[test]
    fn kepler_solver_converges_for_moderate_eccentricity() {
        // 0.26 sits just above Pluto's 0.252, the highest eccentricity any
        // body here is propagated with.
        for &e in &[0.05, 0.10, 0.20, 0.26] {
            for k in 0..16 {
                let m = (k as f64) * (TAU / 16.0);
                let solved = solve_kepler(m, e);
                let residual = solved - e * solved.sin() - m.rem_euclid(TAU);
                assert!(
                    residual.abs() < 1e-8,
                    "residual {residual} too high at e={e}, m={m}"
                );
            }
        }
    }

    #[test]
    fn minor_body_orbits_match_horizons_reference_states() {
        // JPL Horizons heliocentric ECLIPJ2000 positions (AU), 00:00 UT.
        // The circular stand-ins these orbits replaced put Ceres ~2.9 AU off.
        let cases = [
            // 2026-10-08, four months from the elements' epoch: two-body is tight.
            ("CERES", 20_734.0, [0.207_681, 2.659_549, 0.045_937], 1e-3),
            ("VESTA", 20_734.0, [2.321_398, 0.765_362, -0.305_392], 1e-3),
            // 2030-01-01: Jupiter's perturbations accumulate (~0.015 AU measured).
            ("CERES", 21_915.0, [2.825_734, -0.774_188, -0.544_941], 0.03),
        ];
        for (target, unix_days, expected, tolerance_au) in cases {
            let elements = minor_body_elements(target).unwrap();
            let actual = minor_body_orbit(elements, unix_days).position_au();
            let error = (0..3)
                .map(|axis| (actual[axis] - expected[axis]).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!(
                error < tolerance_au,
                "{target} at unix day {unix_days}: {error} AU off Horizons"
            );
        }
    }

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
    fn planet_orbits_match_horizons_reference_states() {
        // JPL Horizons heliocentric ECLIPJ2000 barycentre positions (AU), 00:00 UT,
        // at 2026-10-08 plus the date picker's far ends. The tolerance is an
        // angle along the orbit: the launch-anchored circles these replaced were
        // off by up to 180 deg.
        let cases = [
            (
                "MERCURY BARYCENTER",
                20_734.0,
                [0.133_242, -0.428_363, -0.047_228],
                0.05,
            ),
            (
                "VENUS BARYCENTER",
                20_734.0,
                [0.722_752, 0.058_380, -0.040_899],
                0.05,
            ),
            (
                "EARTH BARYCENTER",
                20_734.0,
                [0.968_102, 0.247_909, -0.000_021],
                0.05,
            ),
            (
                "MARS BARYCENTER",
                20_734.0,
                [0.033_150, 1.566_135, 0.032_008],
                0.1,
            ),
            (
                "JUPITER BARYCENTER",
                20_734.0,
                [-3.530_716, 3.964_587, 0.062_526],
                0.5,
            ),
            (
                "SATURN BARYCENTER",
                20_734.0,
                [9.249_589, 1.804_953, -0.399_624],
                0.5,
            ),
            (
                "URANUS BARYCENTER",
                20_734.0,
                [8.906_029, 17.279_873, -0.051_301],
                0.5,
            ),
            (
                "NEPTUNE BARYCENTER",
                20_734.0,
                [29.836_361, 1.402_219, -0.716_446],
                0.5,
            ),
            (
                "PLUTO BARYCENTER",
                20_734.0,
                [19.982_985, -29.362_562, -2.637_492],
                0.5,
            ),
            (
                "JUPITER BARYCENTER",
                -135_140.0,
                [-4.067_272, 3.466_508, 0.078_330],
                0.5,
            ), // 1600
            (
                "URANUS BARYCENTER",
                -62_091.0,
                [-18.271_165, 0.981_666, 0.242_013],
                0.5,
            ), // 1800
            (
                "SATURN BARYCENTER",
                84_006.0,
                [8.460_607, -4.981_661, -0.253_689],
                0.5,
            ), // 2200
            // Two-body Pluto drifts under Neptune's pull: ~1.9 deg by 1600.
            (
                "PLUTO BARYCENTER",
                -135_140.0,
                [41.029_440, 21.874_063, -14.211_786],
                3.0,
            ),
        ];
        for (target, unix_days, expected, tolerance_deg) in cases {
            let actual = fallback_planet_position_au(target, unix_days);
            let error_au = (0..3)
                .map(|axis| (actual[axis] - expected[axis]).powi(2))
                .sum::<f64>()
                .sqrt();
            let radius_au = expected.iter().map(|c| c * c).sum::<f64>().sqrt();
            let error_deg = (error_au / radius_au).to_degrees();
            assert!(
                error_deg < tolerance_deg,
                "{target} at unix day {unix_days}: {error_deg:.3} deg ({error_au:.4} AU) off Horizons"
            );
        }
    }

    #[test]
    fn j2000_unix_days_is_2000_new_year_noon() {
        let j2000 = chrono::DateTime::from_timestamp((J2000_UNIX_DAYS * 86_400.0) as i64, 0)
            .expect("valid timestamp");
        assert_eq!(j2000.to_rfc3339(), "2000-01-01T12:00:00+00:00");
    }

    #[test]
    fn fallback_position_au_moon_xy_radius_matches_semi_major_axis() {
        let unix_days = 42.0;
        let moon = fallback_position_au("MOON", unix_days);
        let earth = fallback_planet_position_au("EARTH", unix_days);

        let dx = moon[0] - earth[0];
        let dy = moon[1] - earth[1];
        let xy_radius = (dx * dx + dy * dy).sqrt();
        let expected = MOON_SEMI_MAJOR_AXIS_KM / KM_PER_AU;

        assert_close(xy_radius, expected, 1e-12);
    }

    #[test]
    fn satellite_offset_au_keeps_each_moon_at_its_semi_major_axis() {
        for (orbit, unix_days) in [
            (&CHARON_ORBIT, 133.7),
            (&IO_ORBIT, 77.3),
            (&EUROPA_ORBIT, 12.1),
            (&GANYMEDE_ORBIT, 55.0),
            (&CALLISTO_ORBIT, 200.0),
        ] {
            let [dx, dy, _] = satellite_offset_au(orbit, unix_days);
            let expected = orbit.semi_major_axis_km / KM_PER_AU;
            assert_close((dx * dx + dy * dy).sqrt(), expected, 1e-12);
        }
    }

    #[test]
    fn satellite_offset_au_is_prograde_about_ecliptic_north() {
        // Phase advances counter-clockwise seen from +Z (ECLIPJ2000 north),
        // the sense every planet orbits in; `simulation.rs` maps it to scene
        // space with `eclipj2000_to_scene`, which preserves handedness.
        let before = satellite_offset_au(&IO_ORBIT, 10.0);
        let after = satellite_offset_au(&IO_ORBIT, 10.0 + IO_ORBIT.period_days / 100.0);
        let cross_z = before[0] * after[1] - before[1] * after[0];
        assert!(cross_z > 0.0, "phase must advance counter-clockwise");
    }

    #[test]
    fn reconstructed_satellites_are_not_placed_by_the_ephemeris() {
        for target in ["CHARON", "IO", "EUROPA", "GANYMEDE", "CALLISTO"] {
            assert_eq!(fallback_position_au(target, 77.3), [0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn heliocentric_orbit_passes_through_the_body_and_covers_the_sun() {
        let unix_days = 20_734.0;
        for target in [
            "EARTH BARYCENTER",
            "JUPITER BARYCENTER",
            "PLUTO BARYCENTER",
            "CERES",
        ] {
            let orbit = heliocentric_orbit(target, unix_days).unwrap();
            assert_eq!(
                orbit.position_au(),
                fallback_planet_position_au(target, unix_days)
            );
            // Perihelion and aphelion straddle the Sun along the major axis.
            let perihelion = orbit.position_at_mean_anomaly_au(0.0);
            let aphelion = orbit.position_at_mean_anomaly_au(std::f64::consts::PI);
            let r_min = perihelion.iter().map(|c| c * c).sum::<f64>().sqrt();
            let r_max = aphelion.iter().map(|c| c * c).sum::<f64>().sqrt();
            assert_close(
                r_min,
                orbit.semi_major_axis_au * (1.0 - orbit.eccentricity),
                1e-9,
            );
            assert_close(
                r_max,
                orbit.semi_major_axis_au * (1.0 + orbit.eccentricity),
                1e-9,
            );
        }
        for target in [
            "SUN",
            "MOON",
            "IO",
            "CHARON",
            VOYAGER_1_TARGET,
            "NOT_A_REAL_TARGET",
        ] {
            assert!(heliocentric_orbit(target, unix_days).is_none(), "{target}");
        }
    }

    #[test]
    fn voyager_1_position_au_matches_horizons_reference_states() {
        // Horizons heliocentric ECLIPJ2000 positions for target -31.
        let at_epoch = voyager_1_position_au(VOYAGER_1_EPOCH_UNIX_DAYS);
        for (actual, expected) in at_epoch.iter().zip(VOYAGER_1_EPOCH_POSITION_AU) {
            assert_close(*actual, expected, EPS);
        }

        // 2030-01-01, 1461 days on: the fit's other endpoint.
        let later = voyager_1_position_au(VOYAGER_1_EPOCH_UNIX_DAYS + 1461.0);
        let expected = [-33.576_492_774_5, -146.160_740_435_2, 105.737_296_695_3];
        for (actual, expected) in later.iter().zip(expected) {
            assert_close(*actual, expected, 1e-6);
        }
    }

    #[test]
    fn voyager_1_epoch_constant_is_2026_new_year() {
        let epoch =
            chrono::DateTime::from_timestamp((VOYAGER_1_EPOCH_UNIX_DAYS * 86_400.0) as i64, 0)
                .expect("valid timestamp");
        assert_eq!(epoch.to_rfc3339(), "2026-01-01T00:00:00+00:00");
    }

    #[test]
    fn fallback_position_au_unknown_target_defaults_to_origin() {
        assert_eq!(
            fallback_position_au("NOT_A_REAL_TARGET", 12.34),
            [0.0, 0.0, 0.0]
        );
    }

    #[cfg(feature = "spice")]
    #[test]
    fn et_within_coverage_is_inclusive_and_open_without_a_window() {
        assert!(et_within_coverage(None, 1e12));
        assert!(et_within_coverage(Some((-10.0, 10.0)), -10.0));
        assert!(et_within_coverage(Some((-10.0, 10.0)), 10.0));
        assert!(!et_within_coverage(Some((-10.0, 10.0)), 10.5));
        assert!(!et_within_coverage(Some((-10.0, 10.0)), -1e9));
    }

    /// End to end against the committed kernels: SPICE mode comes up, the
    /// launch instant (with its fractional second) parses, the coverage
    /// window is read, and a date outside it falls back instead of aborting.
    #[cfg(feature = "spice")]
    #[test]
    fn spice_mode_loads_the_committed_kernels_and_reads_their_coverage() {
        let spice_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/spice");
        let ephemeris = SpiceEphemeris::new(&spice_dir);
        assert!(ephemeris.is_spice_enabled(), "{}", ephemeris.status_line());

        let unix_days = |y: i32, m: u32, d: u32| {
            let date = chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap();
            date.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() as f64 / SECONDS_PER_DAY
        };
        let (first, last) = ephemeris.spice_coverage_unix_days().expect("coverage");
        // de440s: 1849-12-26 to 2150-01-22, TDB; UTC is about a minute off.
        assert!(
            (first - unix_days(1849, 12, 26)).abs() < 0.01,
            "first {first}"
        );
        assert!((last - unix_days(2150, 1, 22)).abs() < 0.01, "last {last}");

        let radius = |p: [f64; 3]| p.iter().map(|c| c * c).sum::<f64>().sqrt();
        let earth_now = radius(ephemeris.position_au("EARTH", 0.0));
        assert!((0.98..1.02).contains(&earth_now), "Earth at {earth_now} AU");
        // 1700: outside the kernel, so the analytic orbit answers.
        let earth_1700 = radius(ephemeris.position_au("EARTH", unix_days(1700, 1, 1) - first));
        assert!(
            (0.98..1.02).contains(&earth_1700),
            "Earth at {earth_1700} AU"
        );
    }

    #[test]
    fn start_unix_days_is_the_launch_instant() {
        let ephemeris = SpiceEphemeris::new(std::path::Path::new("."));
        let expected = unix_days_of(ephemeris.start_utc());
        assert_eq!(ephemeris.unix_days_at(0.0), expected);
        assert_eq!(ephemeris.unix_days_at(1.5), expected + 1.5);
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
