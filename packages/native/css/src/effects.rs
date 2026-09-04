//! `filter`, `backdrop-filter` and `<blend-mode>`.
//!
//! A filter list folds into one blur, one colour matrix and at most one
//! drop shadow. Every filter function of Filter Effects 1 except `blur()`,
//! `drop-shadow()` and `url()` is a 4 by 5 matrix on straight rgba, and
//! matrices multiply, so a list costs one matrix on the GPU however long it
//! is. Two blurs add in quadrature, since a Gaussian of a Gaussian is a
//! Gaussian. `url()` names an SVG filter, and there is no document to find
//! one in, so it reads as unsupported.

use lightningcss::properties::effects::{
    BlendMode as CssBlendMode, Filter as CssFilter, FilterList,
};
use lightningcss::traits::Parse;
use lightningcss::values::percentage::NumberOrPercentage;

use crate::color::{self, ColorContext, Rgba};
use crate::CssError;

/// A row-major 4 by 5 colour matrix. Rows are the output r, g, b and a.
/// Columns weigh the input r, g, b, a and a constant.
pub type ColorMatrix = [f32; 20];

/// The matrix that changes nothing.
pub const IDENTITY: ColorMatrix = [
    1.0, 0.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 0.0, 1.0, 0.0,
];

/// A filter list, ready for the GPU.
#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    /// Standard deviation of the blur in CSS pixels. 0 is none.
    pub blur: f32,
    /// Every other function, multiplied in list order.
    pub matrix: ColorMatrix,
    /// The one `drop-shadow()` of the list, if it has one.
    pub shadow: Option<DropShadow>,
    /// Whether the shadow colour read `currentColor`.
    pub read_current_color: bool,
}

/// A `drop-shadow()`: the alpha of the element, blurred, moved and coloured,
/// under the element.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropShadow {
    /// The x and y offset in CSS pixels.
    pub offset: (f32, f32),
    /// Standard deviation of the blur in CSS pixels. CSS writes a blur
    /// radius, which is two of these.
    pub blur: f32,
    pub color: Rgba,
}

/// Read a `filter` or `backdrop-filter` value. `none` reads as `Ok(None)`.
pub fn filter(value: &str, context: &ColorContext) -> Result<Option<Filter>, CssError> {
    let list = FilterList::parse_string(value).map_err(|_| CssError::BadValue {
        property: "filter".to_string(),
        value: value.to_string(),
    })?;
    let functions = match list {
        FilterList::None => return Ok(None),
        FilterList::Filters(functions) => functions,
    };
    let mut blur = 0.0f32;
    let mut matrix = IDENTITY;
    let mut shadow = None;
    let mut read_current_color = false;
    for function in functions.iter() {
        match function {
            CssFilter::Blur(length) => {
                let sigma = length.to_px().ok_or_else(|| CssError::Unsupported {
                    feature: "a blur() in a unit that needs the font".to_string(),
                    value: value.to_string(),
                })?;
                blur = (blur * blur + sigma * sigma).sqrt();
            }
            CssFilter::Brightness(amount) => {
                matrix = then(&matrix, &scale(fraction(amount).max(0.0)));
            }
            CssFilter::Contrast(amount) => {
                let amount = fraction(amount).max(0.0);
                matrix = then(&matrix, &linear(amount, 0.5 - 0.5 * amount));
            }
            CssFilter::Grayscale(amount) => {
                matrix = then(&matrix, &grayscale(fraction(amount).clamp(0.0, 1.0)));
            }
            CssFilter::HueRotate(angle) => {
                matrix = then(&matrix, &hue_rotate(angle.to_degrees()));
            }
            CssFilter::Invert(amount) => {
                let amount = fraction(amount).clamp(0.0, 1.0);
                matrix = then(&matrix, &linear(1.0 - 2.0 * amount, amount));
            }
            CssFilter::Opacity(amount) => {
                matrix = then(&matrix, &opacity(fraction(amount).clamp(0.0, 1.0)));
            }
            CssFilter::Saturate(amount) => {
                matrix = then(&matrix, &saturate(fraction(amount).max(0.0)));
            }
            CssFilter::Sepia(amount) => {
                matrix = then(&matrix, &sepia(fraction(amount).clamp(0.0, 1.0)));
            }
            CssFilter::DropShadow(drop) => {
                if shadow.is_some() {
                    return Err(CssError::Unsupported {
                        feature: "more than one drop-shadow()".to_string(),
                        value: value.to_string(),
                    });
                }
                let px = |length: &lightningcss::values::length::Length| {
                    length.to_px().ok_or_else(|| CssError::Unsupported {
                        feature: "a drop-shadow() length in a unit that needs the font"
                            .to_string(),
                        value: value.to_string(),
                    })
                };
                read_current_color |= color::reads_current_color(&drop.color);
                shadow = Some(DropShadow {
                    offset: (px(&drop.x_offset)?, px(&drop.y_offset)?),
                    blur: px(&drop.blur)? / 2.0,
                    color: color::resolve(&drop.color, context)?,
                });
            }
            CssFilter::Url(_) => {
                return Err(CssError::Unsupported {
                    feature: "url() filters".to_string(),
                    value: value.to_string(),
                })
            }
        }
    }
    Ok(Some(Filter {
        blur,
        matrix,
        shadow,
        read_current_color,
    }))
}

/// How a layer mixes with what is under it. Compositing and Blending 1, in
/// spec order, with `plus-lighter` from Compositing 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    PlusLighter,
}

/// Read a `<blend-mode>`. `plus-darker` is Apple only and reads as an error.
pub fn blend_mode(value: &str) -> Result<BlendMode, CssError> {
    let mode = CssBlendMode::parse_string(value).map_err(|_| CssError::BadValue {
        property: "mix-blend-mode".to_string(),
        value: value.to_string(),
    })?;
    Ok(match mode {
        CssBlendMode::Normal => BlendMode::Normal,
        CssBlendMode::Multiply => BlendMode::Multiply,
        CssBlendMode::Screen => BlendMode::Screen,
        CssBlendMode::Overlay => BlendMode::Overlay,
        CssBlendMode::Darken => BlendMode::Darken,
        CssBlendMode::Lighten => BlendMode::Lighten,
        CssBlendMode::ColorDodge => BlendMode::ColorDodge,
        CssBlendMode::ColorBurn => BlendMode::ColorBurn,
        CssBlendMode::HardLight => BlendMode::HardLight,
        CssBlendMode::SoftLight => BlendMode::SoftLight,
        CssBlendMode::Difference => BlendMode::Difference,
        CssBlendMode::Exclusion => BlendMode::Exclusion,
        CssBlendMode::Hue => BlendMode::Hue,
        CssBlendMode::Saturation => BlendMode::Saturation,
        CssBlendMode::Color => BlendMode::Color,
        CssBlendMode::Luminosity => BlendMode::Luminosity,
        CssBlendMode::PlusLighter => BlendMode::PlusLighter,
        CssBlendMode::PlusDarker => {
            return Err(CssError::Unsupported {
                feature: "plus-darker".to_string(),
                value: value.to_string(),
            })
        }
    })
}

fn fraction(amount: &NumberOrPercentage) -> f32 {
    match amount {
        NumberOrPercentage::Number(number) => *number,
        NumberOrPercentage::Percentage(percentage) => percentage.0,
    }
}

/// The matrix that applies `first` and then `second`.
pub fn then(first: &ColorMatrix, second: &ColorMatrix) -> ColorMatrix {
    let mut out = [0.0; 20];
    for row in 0..4 {
        for col in 0..5 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += second[row * 5 + k] * first[k * 5 + col];
            }
            if col == 4 {
                sum += second[row * 5 + 4];
            }
            out[row * 5 + col] = sum;
        }
    }
    out
}

/// A matrix from three colour rows. Alpha passes through.
fn rgb(rows: [[f32; 3]; 3]) -> ColorMatrix {
    let mut out = IDENTITY;
    for (row, weights) in rows.iter().enumerate() {
        out[row * 5] = weights[0];
        out[row * 5 + 1] = weights[1];
        out[row * 5 + 2] = weights[2];
    }
    out
}

/// `c' = slope * c + intercept` on each colour channel.
fn linear(slope: f32, intercept: f32) -> ColorMatrix {
    let mut out = rgb([[slope, 0.0, 0.0], [0.0, slope, 0.0], [0.0, 0.0, slope]]);
    out[4] = intercept;
    out[9] = intercept;
    out[14] = intercept;
    out
}

fn scale(amount: f32) -> ColorMatrix {
    linear(amount, 0.0)
}

fn opacity(amount: f32) -> ColorMatrix {
    let mut out = IDENTITY;
    out[18] = amount;
    out
}

fn grayscale(amount: f32) -> ColorMatrix {
    let a = 1.0 - amount;
    rgb([
        [0.2126 + 0.7874 * a, 0.7152 - 0.7152 * a, 0.0722 - 0.0722 * a],
        [0.2126 - 0.2126 * a, 0.7152 + 0.2848 * a, 0.0722 - 0.0722 * a],
        [0.2126 - 0.2126 * a, 0.7152 - 0.7152 * a, 0.0722 + 0.9278 * a],
    ])
}

fn sepia(amount: f32) -> ColorMatrix {
    let a = 1.0 - amount;
    rgb([
        [0.393 + 0.607 * a, 0.769 - 0.769 * a, 0.189 - 0.189 * a],
        [0.349 - 0.349 * a, 0.686 + 0.314 * a, 0.168 - 0.168 * a],
        [0.272 - 0.272 * a, 0.534 - 0.534 * a, 0.131 + 0.869 * a],
    ])
}

fn saturate(s: f32) -> ColorMatrix {
    rgb([
        [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s],
        [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s],
        [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s],
    ])
}

fn hue_rotate(degrees: f32) -> ColorMatrix {
    let (sin, cos) = degrees.to_radians().sin_cos();
    rgb([
        [
            0.213 + cos * 0.787 - sin * 0.213,
            0.715 - cos * 0.715 - sin * 0.715,
            0.072 - cos * 0.072 + sin * 0.928,
        ],
        [
            0.213 - cos * 0.213 + sin * 0.143,
            0.715 + cos * 0.285 + sin * 0.140,
            0.072 - cos * 0.072 - sin * 0.283,
        ],
        [
            0.213 - cos * 0.213 - sin * 0.787,
            0.715 - cos * 0.715 + sin * 0.715,
            0.072 + cos * 0.928 + sin * 0.072,
        ],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(matrix: &ColorMatrix, c: [f32; 4]) -> [f32; 4] {
        let mut out = [0.0; 4];
        for row in 0..4 {
            out[row] = matrix[row * 5] * c[0]
                + matrix[row * 5 + 1] * c[1]
                + matrix[row * 5 + 2] * c[2]
                + matrix[row * 5 + 3] * c[3]
                + matrix[row * 5 + 4];
        }
        out
    }

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    fn read(value: &str) -> Result<Option<Filter>, CssError> {
        filter(value, &ColorContext::default())
    }

    #[test]
    fn none_reads_as_no_filter() {
        assert_eq!(read("none").unwrap(), None);
    }

    #[test]
    fn a_drop_shadow_reads_its_offset_blur_and_colour() {
        let shadow = read("drop-shadow(2px 4px 6px red)")
            .unwrap()
            .unwrap()
            .shadow
            .unwrap();
        assert_eq!(shadow.offset, (2.0, 4.0));
        // The blur radius is two sigmas.
        assert_eq!(shadow.blur, 3.0);
        assert_eq!(shadow.color, Rgba { r: 1.0, g: 0.0, b: 0.0, a: 1.0 });
    }

    #[test]
    fn a_drop_shadow_without_a_colour_reads_current_color() {
        let context = ColorContext {
            current_color: Rgba { r: 0.0, g: 0.0, b: 1.0, a: 0.5 },
            dark: false,
        };
        let read = filter("drop-shadow(1px 1px)", &context).unwrap().unwrap();
        assert!(read.read_current_color);
        let shadow = read.shadow.unwrap();
        assert_eq!(shadow.blur, 0.0);
        assert_eq!(shadow.color, context.current_color);
    }

    #[test]
    fn a_second_drop_shadow_is_unsupported() {
        assert!(read("drop-shadow(1px 1px red) drop-shadow(2px 2px blue)").is_err());
    }

    #[test]
    fn a_url_filter_is_unsupported() {
        assert!(read("url(#glow)").is_err());
    }

    #[test]
    fn blurs_add_in_quadrature() {
        let read = read("blur(3px) blur(4px)").unwrap().unwrap();
        assert!((read.blur - 5.0).abs() < 1e-5);
        assert_eq!(read.matrix, IDENTITY);
    }

    #[test]
    fn a_full_grayscale_drops_every_hue() {
        let read = read("grayscale(100%)").unwrap().unwrap();
        let red = apply(&read.matrix, [1.0, 0.0, 0.0, 1.0]);
        assert!(close(red, [0.2126, 0.2126, 0.2126, 1.0]), "{red:?}");
    }

    #[test]
    fn invert_flips_the_channels() {
        let read = read("invert(1)").unwrap().unwrap();
        let white = apply(&read.matrix, [1.0, 1.0, 1.0, 1.0]);
        assert!(close(white, [0.0, 0.0, 0.0, 1.0]), "{white:?}");
    }

    #[test]
    fn functions_apply_in_list_order() {
        // Brightness first then invert: 0.5 * 1 = 0.5, then 1 - 0.5 = 0.5.
        // Invert first then brightness: 1 - 1 = 0, then 0.5 * 0 = 0.
        let a = read("brightness(0.5) invert(1)").unwrap().unwrap();
        let b = read("invert(1) brightness(0.5)").unwrap().unwrap();
        let white = [1.0, 1.0, 1.0, 1.0];
        assert!(close(apply(&a.matrix, white), [0.5, 0.5, 0.5, 1.0]));
        assert!(close(apply(&b.matrix, white), [0.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn opacity_scales_alpha_only() {
        let read = read("opacity(25%)").unwrap().unwrap();
        let c = apply(&read.matrix, [0.2, 0.4, 0.6, 1.0]);
        assert!(close(c, [0.2, 0.4, 0.6, 0.25]), "{c:?}");
    }

    #[test]
    fn a_half_turn_of_hue_keeps_the_luminance() {
        let read = read("hue-rotate(180deg)").unwrap().unwrap();
        let grey = apply(&read.matrix, [0.5, 0.5, 0.5, 1.0]);
        assert!(close(grey, [0.5, 0.5, 0.5, 1.0]), "{grey:?}");
    }

    #[test]
    fn an_unknown_function_is_a_bad_value() {
        assert!(matches!(read("wobble(3)"), Err(CssError::BadValue { .. })));
    }

    #[test]
    fn reads_every_blend_mode() {
        assert_eq!(blend_mode("multiply").unwrap(), BlendMode::Multiply);
        assert_eq!(blend_mode("plus-lighter").unwrap(), BlendMode::PlusLighter);
        assert!(matches!(blend_mode("plus-darker"), Err(CssError::Unsupported { .. })));
        assert!(matches!(blend_mode("soft"), Err(CssError::BadValue { .. })));
    }
}
