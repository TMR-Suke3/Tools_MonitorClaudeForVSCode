//! Colour maths for the palette: sRGB, CIE Lab, CIEDE2000, simulated colour-vision
//! deficiency, and WCAG contrast.
//!
//! Every formula here is standard, ported from the phase-3 design spikes that produced the
//! palette in `docs/ui-overlay.md` §3.2. It is deliberately dependency-free and `f64`
//! throughout, so the numbers that document publishes can be reproduced exactly.

/// An 8-bit sRGB colour, as written in `docs/ui-overlay.md`.
pub type Rgb = [u8; 3];

/// CIE L\*a\*b\*, D65.
pub type Lab = [f64; 3];

// D65 white point.
const XN: f64 = 0.950_47;
const YN: f64 = 1.0;
const ZN: f64 = 1.088_83;

// The CIE standard's rational forms, rather than the widely copied 0.008856 / 903.3
// decimal approximations.
const EPSILON: f64 = 216.0 / 24389.0;
const KAPPA: f64 = 24389.0 / 27.0;

/// Parses `#RRGGBB` at compile time, so the palette constants can hold the literal hex
/// strings that `docs/ui-overlay.md` §3.2 publishes.
///
/// # Panics
///
/// At compile time, if `hex` is not `#` followed by exactly six hex digits.
#[must_use]
pub const fn rgb(hex: &str) -> Rgb {
    let b = hex.as_bytes();
    assert!(b.len() == 7 && b[0] == b'#', "expected #RRGGBB");
    [byte(b[1], b[2]), byte(b[3], b[4]), byte(b[5], b[6])]
}

const fn byte(hi: u8, lo: u8) -> u8 {
    nibble(hi) * 16 + nibble(lo)
}

const fn nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("not a hex digit"),
    }
}

/// Formats as `#RRGGBB`, upper case — the form used throughout `docs/`.
#[must_use]
pub fn to_hex(c: Rgb) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

/// sRGB transfer function: an 8-bit encoded value to linear light.
#[must_use]
pub fn to_linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The inverse of [`to_linear`], returning an 8-bit encoded value.
#[must_use]
pub fn from_linear(v: f64) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let encoded = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round().clamp(0.0, 255.0) as u8
}

/// WCAG relative luminance.
#[must_use]
pub fn relative_luminance(c: Rgb) -> f64 {
    0.2126 * to_linear(c[0]) + 0.7152 * to_linear(c[1]) + 0.0722 * to_linear(c[2])
}

/// WCAG contrast ratio between two colours, in the range 1.0 ..= 21.0.
#[must_use]
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Converts sRGB to CIE L\*a\*b\* (D65).
#[must_use]
pub fn lab(c: Rgb) -> Lab {
    let (r, g, b) = (to_linear(c[0]), to_linear(c[1]), to_linear(c[2]));
    let x = r * 0.412_456_4 + g * 0.357_576_1 + b * 0.180_437_5;
    let y = r * 0.212_672_9 + g * 0.715_152_2 + b * 0.072_175_0;
    let z = r * 0.019_333_9 + g * 0.119_192_0 + b * 0.950_304_1;

    fn f(t: f64) -> f64 {
        if t > EPSILON {
            t.cbrt()
        } else {
            (KAPPA * t + 16.0) / 116.0
        }
    }

    let (fx, fy, fz) = (f(x / XN), f(y / YN), f(z / ZN));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIEDE2000 colour difference between two Lab colours, with the unweighted parametric
/// factors `kL = kC = kH = 1`.
#[must_use]
pub fn ciede2000(first: Lab, second: Lab) -> f64 {
    let [l1, a1, b1] = first;
    let [l2, a2, b2] = second;

    let c_bar = (a1.hypot(b1) + a2.hypot(b2)) / 2.0;
    let g = if c_bar > 0.0 {
        0.5 * (1.0 - (c_bar.powi(7) / (c_bar.powi(7) + 25f64.powi(7))).sqrt())
    } else {
        0.5
    };

    let a1p = (1.0 + g) * a1;
    let a2p = (1.0 + g) * a2;
    let c1p = a1p.hypot(b1);
    let c2p = a2p.hypot(b2);
    let h1p = hue_angle(a1p, b1);
    let h2p = hue_angle(a2p, b2);

    let delta_l = l2 - l1;
    let delta_c = c2p - c1p;
    let delta_h_small = if c1p * c2p == 0.0 {
        0.0
    } else {
        let d = h2p - h1p;
        if d > 180.0 {
            d - 360.0
        } else if d < -180.0 {
            d + 360.0
        } else {
            d
        }
    };
    let delta_h = 2.0 * (c1p * c2p).sqrt() * (delta_h_small.to_radians() / 2.0).sin();

    let l_bar = (l1 + l2) / 2.0;
    let c_bar_p = (c1p + c2p) / 2.0;
    let h_bar_p = if c1p * c2p == 0.0 {
        h1p + h2p
    } else {
        let sum = h1p + h2p;
        if (h1p - h2p).abs() > 180.0 {
            if sum < 360.0 {
                (sum + 360.0) / 2.0
            } else {
                (sum - 360.0) / 2.0
            }
        } else {
            sum / 2.0
        }
    };

    let t = 1.0 - 0.17 * (h_bar_p - 30.0).to_radians().cos()
        + 0.24 * (2.0 * h_bar_p).to_radians().cos()
        + 0.32 * (3.0 * h_bar_p + 6.0).to_radians().cos()
        - 0.20 * (4.0 * h_bar_p - 63.0).to_radians().cos();

    let delta_theta = 30.0 * (-(((h_bar_p - 275.0) / 25.0).powi(2))).exp();
    let rc = if c_bar_p > 0.0 {
        2.0 * (c_bar_p.powi(7) / (c_bar_p.powi(7) + 25f64.powi(7))).sqrt()
    } else {
        0.0
    };
    let sl = 1.0 + (0.015 * (l_bar - 50.0).powi(2)) / (20.0 + (l_bar - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * c_bar_p;
    let sh = 1.0 + 0.015 * c_bar_p * t;
    let rt = -(2.0 * delta_theta).to_radians().sin() * rc;

    ((delta_l / sl).powi(2)
        + (delta_c / sc).powi(2)
        + (delta_h / sh).powi(2)
        + rt * (delta_c / sc) * (delta_h / sh))
        .sqrt()
}

fn hue_angle(a: f64, b: f64) -> f64 {
    if a == 0.0 && b == 0.0 {
        0.0
    } else {
        b.atan2(a).to_degrees().rem_euclid(360.0)
    }
}

/// How the board is being looked at. Every threshold in `docs/ui-overlay.md` §3.3 is
/// stated per view, so this is the axis the palette tests iterate over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Vision {
    Normal,
    Protanopia,
    Deuteranopia,
    Tritanopia,
    /// Not a deficiency — a greyscale rendering (a monochrome display, a screenshot, a
    /// printout). It removes the colour channel entirely, which is why §3.3 holds it to
    /// its own pair of thresholds.
    Greyscale,
}

impl Vision {
    /// Every view the palette is judged in.
    pub const ALL: [Vision; 5] = [
        Vision::Normal,
        Vision::Protanopia,
        Vision::Deuteranopia,
        Vision::Tritanopia,
        Vision::Greyscale,
    ];

    /// The views judged against the tier-A / tier-B thresholds — everything except
    /// greyscale, which has its own.
    pub const COLOUR: [Vision; 4] = [
        Vision::Normal,
        Vision::Protanopia,
        Vision::Deuteranopia,
        Vision::Tritanopia,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Vision::Normal => "normal",
            Vision::Protanopia => "protanopia",
            Vision::Deuteranopia => "deuteranopia",
            Vision::Tritanopia => "tritanopia",
            Vision::Greyscale => "greyscale",
        }
    }
}

// Machado, Oliveira & Fernandes (2009), severity 1.0, applied in linear light.
const PROTANOPIA: [[f64; 3]; 3] = [
    [0.152_286, 1.052_583, -0.204_868],
    [0.114_503, 0.786_281, 0.099_216],
    [-0.003_882, -0.048_116, 1.051_998],
];
const DEUTERANOPIA: [[f64; 3]; 3] = [
    [0.367_322, 0.860_646, -0.227_968],
    [0.280_085, 0.672_501, 0.047_413],
    [-0.011_820, 0.042_940, 0.968_881],
];
const TRITANOPIA: [[f64; 3]; 3] = [
    [1.255_528, -0.076_749, -0.178_779],
    [-0.078_411, 0.930_809, 0.147_602],
    [0.004_733, 0.691_367, 0.303_900],
];

/// Renders a colour as it appears in the given view.
#[must_use]
pub fn simulate(c: Rgb, view: Vision) -> Rgb {
    let m = match view {
        Vision::Normal => return c,
        Vision::Greyscale => {
            let v = from_linear(relative_luminance(c));
            return [v, v, v];
        }
        Vision::Protanopia => PROTANOPIA,
        Vision::Deuteranopia => DEUTERANOPIA,
        Vision::Tritanopia => TRITANOPIA,
    };
    let v = [to_linear(c[0]), to_linear(c[1]), to_linear(c[2])];
    let mut out = [0u8; 3];
    for (i, row) in m.iter().enumerate() {
        out[i] = from_linear(row[0] * v[0] + row[1] * v[1] + row[2] * v[2]);
    }
    out
}

/// dE2000 between two sRGB colours as they appear in `view`.
#[must_use]
pub fn difference(a: Rgb, b: Rgb, view: Vision) -> f64 {
    ciede2000(lab(simulate(a, view)), lab(simulate(b, view)))
}

/// Composites `fg` over `bg` at `alpha` in the 8-bit sRGB channel values — what a browser
/// does for CSS `opacity`, and therefore what the board would do
/// ([ADR-0004](../../../docs/adr/0004-use-tauri-and-rust.md): the board is a WebView).
///
/// The board draws every indicator at full opacity
/// ([ADR-0013](../../../docs/adr/0013-make-idle-the-faintest-thing-on-the-board.md) retired
/// the aging ramp). This exists so a test can show what a ramp would have cost.
#[must_use]
pub fn composite_srgb(fg: Rgb, bg: Rgb, alpha: f64) -> Rgb {
    let mut out = [0u8; 3];
    for i in 0..3 {
        let v = f64::from(fg[i]) * alpha + f64::from(bg[i]) * (1.0 - alpha);
        out[i] = v.round().clamp(0.0, 255.0) as u8;
    }
    out
}

/// Composites `fg` over `bg` at `alpha` in linear light: the physically correct model, and
/// the one the design spike measured with.
///
/// It is kept next to [`composite_srgb`] because the two disagree about where an indicator
/// crosses the contrast floor, and the tests assert only what holds under both.
#[must_use]
pub fn composite_linear(fg: Rgb, bg: Rgb, alpha: f64) -> Rgb {
    let mut out = [0u8; 3];
    for i in 0..3 {
        out[i] = from_linear(to_linear(fg[i]) * alpha + to_linear(bg[i]) * (1.0 - alpha));
    }
    out
}
