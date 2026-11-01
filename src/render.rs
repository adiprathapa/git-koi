//! A curved koi colored from its languages on a chosen body, with Classic shading and
//! transparent surroundings.
//!
//! The body and fins follow sampled curves, with shading that follows the body surface
//! and fin rays that follow each fin's direction. The head widens from a round nose
//! along a gentle slant, and short pixel whiskers angle forward from the snout.
//! Contribution totals set the size through [`crate::koi::calculate_growth_fraction`],
//! and growth slows as the fish matures. Young koi have a rounder head, larger eyes, a
//! slimmer body, and short rounded fins, and grow into the adult shape by
//! [`crate::koi::YOUNG_SHAPE_CONTRIBUTIONS`]. After [`crate::koi::ADULT_CONTRIBUTIONS`]
//! the body keeps lengthening, broadens slightly, and its fins and tail lengthen; the
//! maximum form keeps its anatomy at higher totals. Quiet days alone do not change the
//! base anatomy. The pose and framing account for the body and fins.
//!
//! Every growth level shares one wide native canvas of [`FRAME_WIDTH_PIXELS`] by
//! [`FRAME_HEIGHT_PIXELS`], which the encoders in [`crate::output`] scale up exactly.
//! Anatomical proportions live here; pigments, bodies, and palettes in [`crate::coat`];
//! streak mark strengths in [`crate::markings`]; poses in [`crate::motion`].

use std::f64::consts::{PI, TAU};

use crate::behavior::Behavior;
use crate::coat::{KoiBody, KoiCoat, WHITE_RAMP};
use crate::koi::{
    ADULT_CONTRIBUTIONS, KoiDevelopment, YOUNG_SHAPE_CONTRIBUTIONS, calculate_growth_fraction,
};
use crate::motion::{Course, STILL_POSE, SwimPose, calculate_swim_pose, frame_count};
use crate::path::swim_path;

/// Width of the native canvas every koi is drawn on, at every growth level; wide, so a
/// profile README can show it across its full width.
pub const FRAME_WIDTH_PIXELS: u32 = 768;
/// Height of the native canvas.
pub const FRAME_HEIGHT_PIXELS: u32 = 256;

/// A renderer-owned, fixed-size frame with row-major RGBA pixels.
#[derive(Debug, Eq, PartialEq)]
pub struct KoiFrame {
    rgba_pixels: Vec<[u8; 4]>,
}

impl KoiFrame {
    /// The pixels as bytes, row by row from the top left, four channels per pixel: red,
    /// green, blue, alpha. The length is the canvas area times four.
    pub fn as_rgba_bytes(&self) -> &[u8] {
        self.rgba_pixels.as_flattened()
    }

    /// Build a frame from chosen pixels, for encoder tests that need colors no koi has.
    #[cfg(test)]
    pub(crate) fn from_rgba_pixels(rgba_pixels: Vec<[u8; 4]>) -> Self {
        Self { rgba_pixels }
    }
}

/// Render the koi in a centered, fixed swimming pose, colored from its languages on
/// the caller's chosen body. Contribution totals set its size and how far its colors
/// have come in. The account seed places patches in body coordinates, so growth does
/// not reroll the pattern. Streak marks follow the development's streaks as of its
/// observation date. Identical inputs produce identical pixels without I/O, clock
/// reads, or input mutation.
pub fn render_koi(koi_development: &KoiDevelopment, koi_body: KoiBody) -> KoiFrame {
    let koi_anatomy = calculate_anatomy(koi_development.total_contributions);
    render_pose(
        koi_development,
        koi_body,
        &koi_anatomy,
        &STILL_POSE,
        Framing::CenterOnKoi,
    )
}

/// Render one frame of a behavior's seamless loop, colored like `render_koi`, in the
/// pose `motion::calculate_swim_pose` gives; frame numbers wrap around the loop of
/// `motion::frame_count(behavior)` frames. Laps follow the swim course, which wanders
/// the whole frame, and longer koi swim it a little smaller so every size fits. A hovering loop is framed once, centered
/// on the koi's sweep, and its drift shrinks only as far as the largest koi need to
/// stay inside. The view never follows the fins, so it never shakes. Under water the koi
/// turns toward water blue and partly drops out, more with depth. Identical inputs
/// produce identical pixels without I/O, clock reads, or input mutation.
pub fn render_swim_frame(
    koi_development: &KoiDevelopment,
    koi_body: KoiBody,
    behavior: Behavior,
    frame_index: u32,
) -> KoiFrame {
    let koi_anatomy = calculate_anatomy(koi_development.total_contributions);
    let swim_pose = calculate_swim_pose(behavior, frame_index);
    let pose_center = match swim_pose.course {
        Course::Lap { .. } => [0.0, 0.0],
        Course::Hover { drift_pixels, .. } => {
            let hover_fit = fit_hover_loop(behavior, &koi_anatomy);
            [0, 1].map(|axis_index| {
                hover_fit.sweep_center[axis_index]
                    - hover_fit.drift_scales[axis_index] * drift_pixels[axis_index]
            })
        }
    };
    render_pose(
        koi_development,
        koi_body,
        &koi_anatomy,
        &swim_pose,
        Framing::At(pose_center),
    )
}

/// Where the view is centered: on the koi itself for the still image, or at a point
/// fixed for the whole loop.
enum Framing {
    CenterOnKoi,
    At([f64; 2]),
}

/// One framing for a whole hovering loop.
struct HoverFit {
    sweep_center: [f64; 2],
    /// Shares of the pose's drift that keep every frame inside, from zero through one.
    drift_scales: [f64; 2],
}

/// Center a hovering loop on everything the koi covers across its frames, then shrink
/// the drift on each axis only as far as needed to keep every frame inside the frame.
fn fit_hover_loop(behavior: Behavior, koi_anatomy: &KoiAnatomy) -> HoverFit {
    // Room for the outline, a clear pixel at the frame edge, and rounding.
    const FIT_MARGIN_PIXELS: f64 = 2.5;
    let frame_bounds: Vec<([[f64; 2]; 2], [f64; 2])> = (0..frame_count(behavior))
        .map(|frame_index| {
            let swim_pose = calculate_swim_pose(behavior, frame_index);
            let body_spine = build_spine(koi_anatomy, &swim_pose);
            let fin_curves = build_fins(&body_spine, koi_anatomy, &swim_pose);
            let [mut min_corner, mut max_corner] = measure_curve_bounds(&body_spine, &fin_curves);
            // A whisker point covers the pixel to its right and below.
            for whisker_point in
                calculate_whisker_points(&body_spine, koi_anatomy, swim_pose.whisker_sway_radians)
            {
                for axis_index in 0..2 {
                    min_corner[axis_index] = min_corner[axis_index].min(whisker_point[axis_index]);
                    max_corner[axis_index] =
                        max_corner[axis_index].max(whisker_point[axis_index] + 1.0);
                }
            }
            let drift_pixels = match swim_pose.course {
                Course::Hover { drift_pixels, .. } => drift_pixels,
                Course::Lap { .. } => [0.0; 2],
            };
            ([min_corner, max_corner], drift_pixels)
        })
        .collect();
    let frame_sizes = [
        f64::from(FRAME_WIDTH_PIXELS),
        f64::from(FRAME_HEIGHT_PIXELS),
    ];
    let mut hover_fit = HoverFit {
        sweep_center: [0.0; 2],
        drift_scales: [1.0; 2],
    };
    for axis_index in 0..2 {
        let sweep_min = frame_bounds
            .iter()
            .map(|(bounds, _)| bounds[0][axis_index])
            .fold(f64::INFINITY, f64::min);
        let sweep_max = frame_bounds
            .iter()
            .map(|(bounds, _)| bounds[1][axis_index])
            .fold(f64::NEG_INFINITY, f64::max);
        let sweep_center = (sweep_min + sweep_max) / 2.0;
        let frame_half = frame_sizes[axis_index] / 2.0;
        let mut drift_scale: f64 = 1.0;
        for (bounds, drift_pixels) in &frame_bounds {
            let drift = drift_pixels[axis_index];
            let room = if drift > 0.0 {
                frame_half - FIT_MARGIN_PIXELS - (bounds[1][axis_index] - sweep_center)
            } else {
                frame_half - FIT_MARGIN_PIXELS + (bounds[0][axis_index] - sweep_center)
            };
            if drift != 0.0 {
                drift_scale = drift_scale.min(room / drift.abs());
            }
        }
        hover_fit.sweep_center[axis_index] = sweep_center;
        hover_fit.drift_scales[axis_index] = drift_scale.clamp(0.0, 1.0);
    }
    hover_fit
}

fn render_pose(
    koi_development: &KoiDevelopment,
    koi_body: KoiBody,
    koi_anatomy: &KoiAnatomy,
    swim_pose: &SwimPose,
    framing: Framing,
) -> KoiFrame {
    let koi_coat = KoiCoat::new(koi_development, koi_body, |along_fraction| {
        calculate_body_half_width(along_fraction, koi_anatomy)
    });
    let body_spine = build_spine(koi_anatomy, swim_pose);
    let fin_curves = build_fins(&body_spine, koi_anatomy, swim_pose);
    let [min_corner, max_corner] = measure_curve_bounds(&body_spine, &fin_curves);
    let pose_center = match framing {
        Framing::CenterOnKoi => [
            (min_corner[0] + max_corner[0]) / 2.0,
            (min_corner[1] + max_corner[1]) / 2.0,
        ],
        Framing::At(pose_center) => pose_center,
    };
    let koi_appearance = KoiAppearance {
        koi_anatomy,
        koi_coat: &koi_coat,
    };
    let mut rgba_pixels = Vec::with_capacity((FRAME_WIDTH_PIXELS * FRAME_HEIGHT_PIXELS) as usize);
    for pixel_y in 0..FRAME_HEIGHT_PIXELS {
        for pixel_x in 0..FRAME_WIDTH_PIXELS {
            let sample_position = [
                f64::from(pixel_x) + 0.5 - f64::from(FRAME_WIDTH_PIXELS) / 2.0 + pose_center[0],
                f64::from(pixel_y) + 0.5 - f64::from(FRAME_HEIGHT_PIXELS) / 2.0 + pose_center[1],
            ];
            // Nothing outside the curves' bounds can be covered, so skip sampling it.
            let is_inside_bounds = (0..2).all(|axis_index| {
                (min_corner[axis_index]..=max_corner[axis_index])
                    .contains(&sample_position[axis_index])
            });
            rgba_pixels.push(if is_inside_bounds {
                sample_koi(
                    &body_spine,
                    &fin_curves,
                    sample_position,
                    [pixel_x, pixel_y],
                    &koi_appearance,
                )
            } else {
                [0; 4]
            });
        }
    }
    paint_mouth(
        &mut rgba_pixels,
        &body_spine,
        koi_body,
        pose_center,
        swim_pose.mouth_opening,
    );
    let fish_pixel_mask: Vec<bool> = rgba_pixels
        .iter()
        .map(|pixel_channels| pixel_channels[3] != 0)
        .collect();
    let whisker_points =
        calculate_whisker_points(&body_spine, koi_anatomy, swim_pose.whisker_sway_radians);
    paint_whiskers(&mut rgba_pixels, &whisker_points, pose_center);
    paint_outline(&mut rgba_pixels);
    if let Some(feeding_phase) = swim_pose.feeding_phase {
        paint_feeding(
            &mut rgba_pixels,
            &body_spine,
            koi_anatomy,
            pose_center,
            feeding_phase,
        );
    }
    if swim_pose.depth > 0.0 {
        paint_depth(&mut rgba_pixels, &fish_pixel_mask, swim_pose.depth);
    }
    KoiFrame { rgba_pixels }
}

/// Where the mouth opens: on the snout's center line, just inside its front.
fn calculate_mouth_position(body_spine: &[CurvePoint]) -> [f64; 2] {
    const MOUTH_FORWARD_FRACTION: f64 = 0.75;
    let snout = body_spine[0];
    let forward_direction = calculate_forward_direction(body_spine, 0);
    [0, 1].map(|axis_index| {
        snout.position[axis_index]
            + forward_direction[axis_index] * snout.half_width_pixels * MOUTH_FORWARD_FRACTION
    })
}

/// The mouth: a circle at the front of the snout with a rim in the body's color around
/// a dark opening, growing from under a pixel to half the snout's width as it opens.
/// It is painted before the outline, so it is outlined like the koi, and a mostly
/// closed mouth paints nothing.
fn paint_mouth(
    rgba_pixels: &mut [[u8; 4]],
    body_spine: &[CurvePoint],
    koi_body: KoiBody,
    pose_center: [f64; 2],
    mouth_opening: f64,
) {
    const VISIBLE_OPENING: f64 = 0.1;
    const CLOSED_RADIUS_PIXELS: f64 = 0.8;
    const OPEN_RADIUS_PER_SNOUT_RADIUS: f64 = 0.5;
    const MIN_OPEN_RADIUS_PIXELS: f64 = 1.8;
    const RIM_WIDTH_PIXELS: f64 = 0.9;
    if mouth_opening <= VISIBLE_OPENING {
        return;
    }
    let open_radius_pixels = (body_spine[0].half_width_pixels * OPEN_RADIUS_PER_SNOUT_RADIUS)
        .max(MIN_OPEN_RADIUS_PIXELS);
    let radius_pixels = interpolate(
        CLOSED_RADIUS_PIXELS,
        open_radius_pixels,
        mouth_opening.min(1.0),
    );
    let mouth_world = calculate_mouth_position(body_spine);
    let frame_sizes = [
        f64::from(FRAME_WIDTH_PIXELS),
        f64::from(FRAME_HEIGHT_PIXELS),
    ];
    let mouth = [0, 1].map(|axis_index| {
        mouth_world[axis_index] - pose_center[axis_index] + frame_sizes[axis_index] / 2.0
    });
    let rim_color = koi_body.select_mouth_rim_color();
    let frame_width = FRAME_WIDTH_PIXELS as usize;
    let first_y = (mouth[1] - radius_pixels).floor().max(0.0) as usize;
    let last_y = ((mouth[1] + radius_pixels).ceil() as usize).min(FRAME_HEIGHT_PIXELS as usize - 1);
    let first_x = (mouth[0] - radius_pixels).floor().max(0.0) as usize;
    let last_x = ((mouth[0] + radius_pixels).ceil() as usize).min(frame_width - 1);
    for pixel_y in first_y..=last_y {
        for pixel_x in first_x..=last_x {
            let distance_pixels =
                (pixel_x as f64 + 0.5 - mouth[0]).hypot(pixel_y as f64 + 0.5 - mouth[1]);
            if distance_pixels <= radius_pixels {
                rgba_pixels[pixel_y * frame_width + pixel_x] =
                    if distance_pixels > radius_pixels - RIM_WIDTH_PIXELS {
                        rim_color
                    } else {
                        OUTLINE_COLOR
                    };
            }
        }
    }
}

/// Surface feeding: ripple rings spreading from the mouth and food pellets drifting in
/// to it, drawn on the water around the koi. Where the frame edge leaves too little
/// room, both shrink toward the mouth instead of running off the edge.
fn paint_feeding(
    rgba_pixels: &mut [[u8; 4]],
    body_spine: &[CurvePoint],
    koi_anatomy: &KoiAnatomy,
    pose_center: [f64; 2],
    feeding_phase: f64,
) {
    const RING_COUNT: usize = 3;
    const RING_WIDTH_PER_BODY_LENGTH: f64 = 0.3;
    const RING_HEIGHT_PER_WIDTH: f64 = 0.4;
    const RING_ANGLE_STEP_RADIANS: f64 = 0.035;
    const PELLET_COUNT: usize = 5;
    const PELLET_NEAREST_PIXELS: f64 = 4.0;
    const PELLET_REACH_PIXELS: f64 = 14.0;
    const PELLET_WANDER_PIXELS: f64 = 9.0;
    // Pellets vanish just before they reach the mouth.
    const PELLET_EATEN_LIFE: f64 = 0.92;
    const PELLET_COLORS: [[u8; 4]; 2] = [[228, 166, 114, 255], [184, 111, 80, 255]];
    // A clear pixel between the extras and the frame edge, plus rounding.
    const EDGE_MARGIN_PIXELS: f64 = 2.0;
    let frame_sizes = [
        f64::from(FRAME_WIDTH_PIXELS),
        f64::from(FRAME_HEIGHT_PIXELS),
    ];
    let mouth_world = calculate_mouth_position(body_spine);
    let mouth = [0, 1].map(|axis_index| {
        mouth_world[axis_index] - pose_center[axis_index] + frame_sizes[axis_index] / 2.0
    });
    let room = [0, 1].map(|axis_index| {
        (mouth[axis_index] - EDGE_MARGIN_PIXELS)
            .min(frame_sizes[axis_index] - 1.0 - EDGE_MARGIN_PIXELS - mouth[axis_index])
            .max(0.0)
    });
    let forward_direction = calculate_forward_direction(body_spine, 0);
    let frame_width = FRAME_WIDTH_PIXELS as usize;
    let mut paint_water = |point: [f64; 2], color: [u8; 4], dither_limit: f64| {
        let [pixel_x, pixel_y] = point.map(f64::floor);
        if !(0.0..frame_sizes[0]).contains(&pixel_x) || !(0.0..frame_sizes[1]).contains(&pixel_y) {
            return;
        }
        let pixel_index = pixel_y as usize * frame_width + pixel_x as usize;
        if rgba_pixels[pixel_index][3] == 0
            && calculate_dither_threshold([pixel_x as u32, pixel_y as u32]) < dither_limit
        {
            rgba_pixels[pixel_index] = color;
        }
    };
    let largest_ring_width = koi_anatomy.body_length_pixels * RING_WIDTH_PER_BODY_LENGTH * 0.85;
    let ring_scale = 1.0_f64
        .min(room[0] / largest_ring_width)
        .min(room[1] / (largest_ring_width * RING_HEIGHT_PER_WIDTH));
    for ring_index in 0..RING_COUNT {
        let ring_phase = (feeding_phase + ring_index as f64 / RING_COUNT as f64).fract();
        let ring_width = ring_scale
            * koi_anatomy.body_length_pixels
            * RING_WIDTH_PER_BODY_LENGTH
            * (0.25 + 0.6 * ring_phase);
        let ring_color = WHITE_RAMP[if ring_index % 2 == 1 { 2 } else { 1 }];
        let mut ring_angle: f64 = 0.0;
        while ring_angle < TAU {
            paint_water(
                [
                    mouth[0] + ring_angle.cos() * ring_width,
                    mouth[1] + ring_angle.sin() * ring_width * RING_HEIGHT_PER_WIDTH,
                ],
                ring_color,
                (1.0 - ring_phase) * 0.75,
            );
            ring_angle += RING_ANGLE_STEP_RADIANS;
        }
    }
    // The second pellet pixel sits one to the right, so leave it room too.
    let farthest_pellet_pixels = PELLET_NEAREST_PIXELS + PELLET_REACH_PIXELS + PELLET_WANDER_PIXELS;
    let pellet_scale = 1.0_f64.min((room[0].min(room[1]) - 1.0).max(0.0) / farthest_pellet_pixels);
    for pellet_index in 0..PELLET_COUNT {
        let life = (feeding_phase + pellet_index as f64 / PELLET_COUNT as f64).fract();
        if life >= PELLET_EATEN_LIFE {
            continue;
        }
        let reach = PELLET_NEAREST_PIXELS + (1.0 - life) * PELLET_REACH_PIXELS;
        let wander = (1.0 - life) * PELLET_WANDER_PIXELS;
        let pellet_point = [
            mouth[0]
                + pellet_scale
                    * (forward_direction[0] * reach + (pellet_index as f64 * 2.1).sin() * wander),
            mouth[1]
                + pellet_scale
                    * (forward_direction[1] * reach + (pellet_index as f64 * 1.7).cos() * wander),
        ];
        paint_water(pellet_point, PELLET_COLORS[0], 1.0);
        paint_water(
            [pellet_point[0] + 1.0, pellet_point[1]],
            PELLET_COLORS[1],
            1.0,
        );
    }
}

/// Under water the koi becomes a blue koi: each body and fin pixel maps its own
/// brightness onto a water-blue scale, the whiskers and outline soften to deep blue,
/// and a dithered share of each drops out, more for the outline.
fn paint_depth(rgba_pixels: &mut [[u8; 4]], fish_pixel_mask: &[bool], depth: f64) {
    const DEEP_WATER: [f64; 3] = [18.0, 78.0, 137.0];
    const MID_WATER: [f64; 3] = [0.0, 153.0, 219.0];
    const SHALLOW_WATER: [f64; 3] = [190.0, 225.0, 245.0];
    const OUTLINE_WATER: [f64; 3] = [24.0, 48.0, 92.0];
    const FISH_DROPOUT_PER_DEPTH: f64 = 0.15;
    const OUTLINE_DROPOUT_PER_DEPTH: f64 = 0.45;
    const TINT_PER_DEPTH: f64 = 1.15;
    let tint = (depth * TINT_PER_DEPTH).min(1.0);
    let frame_width = FRAME_WIDTH_PIXELS as usize;
    for (pixel_index, pixel_channels) in rgba_pixels.iter_mut().enumerate() {
        if pixel_channels[3] == 0 {
            continue;
        }
        let is_fish_pixel = fish_pixel_mask[pixel_index];
        let dropout_per_depth = if is_fish_pixel {
            FISH_DROPOUT_PER_DEPTH
        } else {
            OUTLINE_DROPOUT_PER_DEPTH
        };
        let pixel_position = [
            (pixel_index % frame_width) as u32,
            (pixel_index / frame_width) as u32,
        ];
        if calculate_dither_threshold(pixel_position) < depth * dropout_per_depth {
            *pixel_channels = [0; 4];
            continue;
        }
        let original_color =
            [0, 1, 2].map(|channel_index| f64::from(pixel_channels[channel_index]));
        let water_color = if is_fish_pixel {
            let brightness =
                (0.3 * original_color[0] + 0.59 * original_color[1] + 0.11 * original_color[2])
                    / 255.0;
            let (from_color, to_color, blend) = if brightness < 0.5 {
                (DEEP_WATER, MID_WATER, brightness * 2.0)
            } else {
                (MID_WATER, SHALLOW_WATER, brightness * 2.0 - 1.0)
            };
            [0, 1, 2].map(|channel_index| {
                interpolate(from_color[channel_index], to_color[channel_index], blend)
            })
        } else {
            OUTLINE_WATER
        };
        for channel_index in 0..3 {
            pixel_channels[channel_index] = interpolate(
                original_color[channel_index],
                water_color[channel_index],
                tint,
            )
            .round() as u8;
        }
    }
}

const OUTLINE_COLOR: [u8; 4] = [24, 20, 37, 255];
const DITHER_RANKS: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

struct KoiAnatomy {
    body_length_pixels: f64,
    body_width_pixels: f64,
    /// Share of the body length over which the head widens from the nose.
    head_slant_fraction: f64,
    /// Nose width as a share of the full body width.
    nose_width_fraction: f64,
    /// Exponent of the taper from the shoulders to the tail stalk.
    tail_taper_exponent: f64,
    /// Fin shapes blend from young (0) to adult (1).
    adult_fin_fraction: f64,
    fin_length_multiplier: f64,
    eye_along_fraction: f64,
    /// Young koi have proportionally larger eyes; adults use 1.
    eye_scale: f64,
}

struct KoiAppearance<'a> {
    koi_anatomy: &'a KoiAnatomy,
    koi_coat: &'a KoiCoat,
}

// Positions and half widths are pixels; consecutive points form tapered strokes.
#[derive(Clone, Copy)]
struct CurvePoint {
    position: [f64; 2],
    half_width_pixels: f64,
}

struct CurveSample {
    along_fraction: f64,
    lateral_pixels: f64,
    distance_pixels: f64,
    half_width_pixels: f64,
    normal_direction: [f64; 2],
}

#[derive(Clone, Copy)]
struct FinShape {
    length_body_widths: f64,
    half_width_body_widths: f64,
    width_start_fraction: f64,
    taper_exponent: f64,
}

impl FinShape {
    fn blend(self, other: Self, other_fraction: f64) -> Self {
        Self {
            length_body_widths: interpolate(
                self.length_body_widths,
                other.length_body_widths,
                other_fraction,
            ),
            half_width_body_widths: interpolate(
                self.half_width_body_widths,
                other.half_width_body_widths,
                other_fraction,
            ),
            width_start_fraction: interpolate(
                self.width_start_fraction,
                other.width_start_fraction,
                other_fraction,
            ),
            taper_exponent: interpolate(self.taper_exponent, other.taper_exponent, other_fraction),
        }
    }
}

// Each pair shares its proportions: short rounded fins when young, longer adult fins.
// Direction offsets in radians describe the two fins independently, relative to the
// spine's backward tangent.
struct FinPair {
    fin_kind: FinKind,
    root_along_fraction: f64,
    root_side_fraction: f64,
    young_shape: FinShape,
    adult_shape: FinShape,
    direction_offsets: [&'static [f64]; 2],
}

#[derive(Clone, Copy)]
enum FinKind {
    Tail,
    Side,
    Pelvic,
}

const FIN_PAIRS: [FinPair; 3] = [
    FinPair {
        fin_kind: FinKind::Tail,
        root_along_fraction: 1.0,
        root_side_fraction: 0.0,
        young_shape: FinShape {
            length_body_widths: 1.05,
            half_width_body_widths: 0.3,
            width_start_fraction: 0.16,
            taper_exponent: 0.6,
        },
        adult_shape: FinShape {
            length_body_widths: 1.62,
            half_width_body_widths: 0.3,
            width_start_fraction: 0.125,
            taper_exponent: 0.69,
        },
        direction_offsets: [
            &[-0.18035, -0.25648, -0.37367, -0.50421, -0.61344, -0.66997],
            &[0.248, 0.30115, 0.29856, 0.24064, 0.14695, 0.05079],
        ],
    },
    FinPair {
        fin_kind: FinKind::Side,
        root_along_fraction: 0.22,
        root_side_fraction: 0.8,
        young_shape: FinShape {
            length_body_widths: 0.66,
            half_width_body_widths: 0.25,
            width_start_fraction: 0.18,
            taper_exponent: 0.45,
        },
        adult_shape: FinShape {
            length_body_widths: 1.13,
            half_width_body_widths: 0.24,
            width_start_fraction: 0.09,
            taper_exponent: 0.54,
        },
        direction_offsets: [
            &[0.87, 0.77239, 0.71583, 0.70044],
            &[-0.65221, -0.58002, -0.5356, -0.48311],
        ],
    },
    FinPair {
        fin_kind: FinKind::Pelvic,
        root_along_fraction: 0.47,
        root_side_fraction: 0.7,
        young_shape: FinShape {
            length_body_widths: 0.36,
            half_width_body_widths: 0.1,
            width_start_fraction: 0.15,
            taper_exponent: 0.5,
        },
        adult_shape: FinShape {
            length_body_widths: 0.57,
            half_width_body_widths: 0.11,
            width_start_fraction: 0.11,
            taper_exponent: 0.59,
        },
        direction_offsets: [&[0.39139, 0.3769, 0.39376], &[-0.62997, -0.48164, -0.32809]],
    },
];

fn calculate_anatomy(total_contributions: u64) -> KoiAnatomy {
    const MIN_BODY_LENGTH_PIXELS: f64 = 37.5;
    const ADULT_BODY_LENGTH_PIXELS: f64 = 137.5;
    const MAX_BODY_LENGTH_PIXELS: f64 = 167.5;
    const YOUNG_BODY_WIDTH_FRACTION: f64 = 0.17;
    const ADULT_BODY_WIDTH_FRACTION: f64 = 0.2;
    const MATURE_BODY_WIDTH_GAIN: f64 = 0.045;
    const YOUNG_HEAD_SLANT_FRACTION: f64 = 0.14;
    const ADULT_HEAD_SLANT_FRACTION: f64 = 0.187;
    const YOUNG_NOSE_WIDTH_FRACTION: f64 = 0.75;
    const ADULT_NOSE_WIDTH_FRACTION: f64 = 0.517;
    const YOUNG_TAIL_TAPER_EXPONENT: f64 = 1.25;
    const ADULT_TAIL_TAPER_EXPONENT: f64 = 1.8;
    const YOUNG_EYE_ALONG_FRACTION: f64 = 0.08;
    const ADULT_EYE_ALONG_FRACTION: f64 = 0.06;
    const YOUNG_EYE_SCALE: f64 = 2.1;
    const YOUNG_FIN_LENGTH_MULTIPLIER: f64 = 0.85;
    const MAX_FIN_LENGTH_MULTIPLIER: f64 = 1.1;
    // Fins keep their young shape for the first quarter of the young stage.
    const YOUNG_FIN_HOLD_FRACTION: f64 = 0.25;
    let growth_fraction = calculate_growth_fraction(total_contributions);
    let adult_growth_fraction = calculate_growth_fraction(ADULT_CONTRIBUTIONS);
    let young_growth_fraction = (growth_fraction / adult_growth_fraction).min(1.0);
    let mature_growth_fraction =
        ((growth_fraction - adult_growth_fraction) / (1.0 - adult_growth_fraction)).max(0.0);
    let adult_shape_fraction =
        (growth_fraction / calculate_growth_fraction(YOUNG_SHAPE_CONTRIBUTIONS)).min(1.0);
    let body_length_pixels = MIN_BODY_LENGTH_PIXELS
        + (ADULT_BODY_LENGTH_PIXELS - MIN_BODY_LENGTH_PIXELS) * young_growth_fraction
        + (MAX_BODY_LENGTH_PIXELS - ADULT_BODY_LENGTH_PIXELS) * mature_growth_fraction;
    let body_width_fraction = interpolate(
        YOUNG_BODY_WIDTH_FRACTION,
        ADULT_BODY_WIDTH_FRACTION,
        adult_shape_fraction,
    ) + MATURE_BODY_WIDTH_GAIN * mature_growth_fraction;
    KoiAnatomy {
        body_length_pixels,
        body_width_pixels: body_length_pixels * body_width_fraction,
        head_slant_fraction: interpolate(
            YOUNG_HEAD_SLANT_FRACTION,
            ADULT_HEAD_SLANT_FRACTION,
            adult_shape_fraction,
        ),
        nose_width_fraction: interpolate(
            YOUNG_NOSE_WIDTH_FRACTION,
            ADULT_NOSE_WIDTH_FRACTION,
            adult_shape_fraction,
        ),
        tail_taper_exponent: interpolate(
            YOUNG_TAIL_TAPER_EXPONENT,
            ADULT_TAIL_TAPER_EXPONENT,
            adult_shape_fraction,
        ),
        adult_fin_fraction: smooth_step(
            (adult_shape_fraction - YOUNG_FIN_HOLD_FRACTION) / (1.0 - YOUNG_FIN_HOLD_FRACTION),
        ),
        fin_length_multiplier: interpolate(
            YOUNG_FIN_LENGTH_MULTIPLIER,
            1.0,
            smooth_step(adult_shape_fraction),
        ) * interpolate(
            1.0,
            MAX_FIN_LENGTH_MULTIPLIER,
            mature_growth_fraction,
        ),
        eye_along_fraction: interpolate(
            YOUNG_EYE_ALONG_FRACTION,
            ADULT_EYE_ALONG_FRACTION,
            adult_shape_fraction,
        ),
        eye_scale: interpolate(YOUNG_EYE_SCALE, 1.0, adult_shape_fraction),
    }
}

fn interpolate(from: f64, to: f64, fraction: f64) -> f64 {
    from + (to - from) * fraction
}

/// Ease in and out across zero through one; values outside are clamped.
fn smooth_step(fraction: f64) -> f64 {
    let fraction = fraction.clamp(0.0, 1.0);
    fraction * fraction * (3.0 - 2.0 * fraction)
}

/// The spine follows the pose's course: the still pose's fixed heading and gentle bend,
/// turned by any heading offset, or the swim course around the frame.
fn build_spine(koi_anatomy: &KoiAnatomy, swim_pose: &SwimPose) -> Vec<CurvePoint> {
    const HEADING_RADIANS: f64 = -0.08;
    const BEND_RADIANS: f64 = 0.35;
    const WAVE_AMPLITUDE_FRACTION: f64 = 0.06;
    const WAVE_CYCLES: f64 = 0.9;
    let body_length_pixels = koi_anatomy.body_length_pixels;
    let point_count = body_length_pixels.round() as usize;
    const MIN_HALF_WIDTH_PIXELS: f64 = 1.3;
    let calculate_point_half_width = |point_index: usize| {
        calculate_body_half_width(point_index as f64 / point_count as f64, koi_anatomy)
            .max(MIN_HALF_WIDTH_PIXELS)
    };
    let center_points = match swim_pose.course {
        Course::Hover {
            heading_offset_radians,
            ..
        } => {
            let mut center_points = vec![CurvePoint {
                position: [0.0, 0.0],
                half_width_pixels: calculate_point_half_width(0),
            }];
            for point_index in 1..point_count {
                let backward_angle_radians = HEADING_RADIANS
                    + heading_offset_radians
                    + PI
                    + BEND_RADIANS * point_index as f64 / point_count as f64;
                let [previous_x, previous_y] = center_points[point_index - 1].position;
                center_points.push(CurvePoint {
                    position: [
                        previous_x + backward_angle_radians.cos(),
                        previous_y + backward_angle_radians.sin(),
                    ],
                    half_width_pixels: calculate_point_half_width(point_index),
                });
            }
            center_points
        }
        // Each body point sits one pixel of arc behind the one in front.
        Course::Lap { course_fraction } => {
            let swim_path = swim_path().fit_to_margin(calculate_course_margin(body_length_pixels));
            let distance_pixels = course_fraction * swim_path.length_pixels();
            (0..point_count)
                .map(|point_index| CurvePoint {
                    position: swim_path.position_at(distance_pixels - point_index as f64),
                    half_width_pixels: calculate_point_half_width(point_index),
                })
                .collect()
        }
    };
    let mut body_spine = Vec::with_capacity(point_count);
    for (point_index, center_point) in center_points.iter().enumerate() {
        let along_fraction = point_index as f64 / point_count as f64;
        let forward_direction = calculate_forward_direction(&center_points, point_index);
        let head_wave_fraction = 0.1;
        let wave_envelope =
            head_wave_fraction + (1.0 - head_wave_fraction) * along_fraction.powi(2);
        let wave_offset = body_length_pixels
            * WAVE_AMPLITUDE_FRACTION
            * swim_pose.wave_strength
            * wave_envelope
            * (along_fraction * TAU * WAVE_CYCLES - swim_pose.wave_phase_radians).sin();
        body_spine.push(CurvePoint {
            position: [
                center_point.position[0] - forward_direction[1] * wave_offset,
                center_point.position[1] + forward_direction[0] * wave_offset,
            ],
            half_width_pixels: center_point.half_width_pixels,
        });
    }
    body_spine
}

/// Each fin turns at its root and bends along its length as the pose says; a turn
/// outward is away from the body, whichever side the fin is on.
fn build_fins(
    body_spine: &[CurvePoint],
    koi_anatomy: &KoiAnatomy,
    swim_pose: &SwimPose,
) -> Vec<Vec<CurvePoint>> {
    let mut fin_curves = Vec::new();
    for fin_pair in &FIN_PAIRS {
        let root_index =
            (fin_pair.root_along_fraction * (body_spine.len() - 1) as f64).round() as usize;
        let root_point = body_spine[root_index];
        let forward_direction = calculate_forward_direction(body_spine, root_index);
        let backward_angle_radians = (-forward_direction[1]).atan2(-forward_direction[0]);
        for (side_index, (body_side, direction_offsets)) in [-1.0, 1.0]
            .into_iter()
            .zip(fin_pair.direction_offsets)
            .enumerate()
        {
            let outward_sign = direction_offsets[0].signum();
            let (root_turn_radians, bend_radians) = match fin_pair.fin_kind {
                FinKind::Tail => (
                    outward_sign * swim_pose.tail_spread_radians,
                    swim_pose.tail_lobe_bends_radians[side_index],
                ),
                FinKind::Side => (
                    outward_sign * swim_pose.side_fin_openings_radians[side_index],
                    outward_sign * swim_pose.side_fin_bends_radians[side_index],
                ),
                FinKind::Pelvic => (outward_sign * swim_pose.pelvic_fin_opening_radians, 0.0),
            };
            let root_offset_pixels =
                root_point.half_width_pixels * fin_pair.root_side_fraction * body_side;
            let root_position = [
                root_point.position[0] - forward_direction[1] * root_offset_pixels,
                root_point.position[1] + forward_direction[0] * root_offset_pixels,
            ];
            fin_curves.push(build_fin(
                fin_pair,
                root_position,
                backward_angle_radians + root_turn_radians,
                koi_anatomy,
                direction_offsets,
                bend_radians,
            ));
        }
    }
    fin_curves
}

fn build_fin(
    fin_pair: &FinPair,
    root_position: [f64; 2],
    backward_angle_radians: f64,
    koi_anatomy: &KoiAnatomy,
    direction_offsets: &[f64],
    bend_radians: f64,
) -> Vec<CurvePoint> {
    let fin_shape = fin_pair
        .young_shape
        .blend(fin_pair.adult_shape, koi_anatomy.adult_fin_fraction);
    let fin_length_pixels = koi_anatomy.body_width_pixels
        * fin_shape.length_body_widths
        * koi_anatomy.fin_length_multiplier;
    let segment_count = fin_length_pixels.ceil() as usize;
    let segment_length_pixels = fin_length_pixels / segment_count as f64;
    let mut center_position = root_position;
    let mut fin_points = Vec::new();
    for point_index in 0..=segment_count {
        let along_fraction = point_index as f64 / segment_count as f64;
        let width_progress = fin_shape.width_start_fraction
            + (1.0 - fin_shape.width_start_fraction) * along_fraction;
        fin_points.push(CurvePoint {
            position: center_position,
            half_width_pixels: koi_anatomy.body_width_pixels
                * fin_shape.half_width_body_widths
                * (PI * width_progress)
                    .sin()
                    .max(0.0)
                    .powf(fin_shape.taper_exponent),
        });
        let direction_position = (along_fraction * direction_offsets.len() as f64 - 0.5)
            .clamp(0.0, (direction_offsets.len() - 1) as f64);
        let direction_index = direction_position.floor() as usize;
        let start_angle = direction_offsets[direction_index];
        let end_angle = direction_offsets[(direction_index + 1).min(direction_offsets.len() - 1)];
        let segment_angle = backward_angle_radians
            + start_angle
            + (end_angle - start_angle) * direction_position.fract()
            + bend_radians * along_fraction;
        center_position[0] += segment_length_pixels * segment_angle.cos();
        center_position[1] += segment_length_pixels * segment_angle.sin();
    }
    fin_points
}

/// Paint the whisker points. They fill only empty pixels and are painted before the
/// outline, so they are outlined like the koi.
fn paint_whiskers(rgba_pixels: &mut [[u8; 4]], whisker_points: &[[f64; 2]], pose_center: [f64; 2]) {
    let frame_width = FRAME_WIDTH_PIXELS as usize;
    for whisker_point in whisker_points {
        let pixel_x =
            (whisker_point[0] - pose_center[0] + f64::from(FRAME_WIDTH_PIXELS) / 2.0).floor();
        let pixel_y =
            (whisker_point[1] - pose_center[1] + f64::from(FRAME_HEIGHT_PIXELS) / 2.0).floor();
        if !(0.0..f64::from(FRAME_WIDTH_PIXELS)).contains(&pixel_x)
            || !(0.0..f64::from(FRAME_HEIGHT_PIXELS)).contains(&pixel_y)
        {
            continue;
        }
        let pixel_index = pixel_y as usize * frame_width + pixel_x as usize;
        if rgba_pixels[pixel_index][3] == 0 {
            rgba_pixels[pixel_index] = WHITE_RAMP[1];
        }
    }
}

/// One point per whisker pixel: short lines angled forward and out from the edge of
/// the snout, swung together by the sway.
fn calculate_whisker_points(
    body_spine: &[CurvePoint],
    koi_anatomy: &KoiAnatomy,
    sway_radians: f64,
) -> Vec<[f64; 2]> {
    const SPREAD_RADIANS: f64 = 0.85;
    const ROOT_SIDE_FRACTION: f64 = 0.8;
    const ROOT_FORWARD_FRACTION: f64 = 0.55;
    let whisker_length_pixels = calculate_whisker_length(koi_anatomy);
    let snout_position = body_spine[0].position;
    let snout_radius_pixels = body_spine[0].half_width_pixels;
    let forward_direction = calculate_forward_direction(body_spine, 0);
    let heading_radians = forward_direction[1].atan2(forward_direction[0]);
    let mut whisker_points = Vec::new();
    for body_side in [-1.0, 1.0] {
        let side_offset_pixels = snout_radius_pixels * ROOT_SIDE_FRACTION * body_side;
        let forward_offset_pixels = snout_radius_pixels * ROOT_FORWARD_FRACTION;
        let root_position = [
            snout_position[0] - forward_direction[1] * side_offset_pixels
                + forward_direction[0] * forward_offset_pixels,
            snout_position[1]
                + forward_direction[0] * side_offset_pixels
                + forward_direction[1] * forward_offset_pixels,
        ];
        let whisker_angle = heading_radians + sway_radians + body_side * SPREAD_RADIANS;
        for step_pixels in 1..=whisker_length_pixels {
            whisker_points.push([
                root_position[0] + whisker_angle.cos() * f64::from(step_pixels),
                root_position[1] + whisker_angle.sin() * f64::from(step_pixels),
            ]);
        }
    }
    whisker_points
}

/// Whiskers grow with the koi, from three to five pixels.
fn calculate_whisker_length(koi_anatomy: &KoiAnatomy) -> u32 {
    const BODY_PIXELS_PER_WHISKER_PIXEL: f64 = 34.0;
    (koi_anatomy.body_length_pixels / BODY_PIXELS_PER_WHISKER_PIXEL)
        .round()
        .clamp(3.0, 5.0) as u32
}

/// Room the swim course keeps from the frame edges: enough for the body, fins, and a
/// tail swung wide in a turn, so longer koi swim a slightly smaller course.
fn calculate_course_margin(body_length_pixels: f64) -> f64 {
    const MARGIN_PER_BODY_LENGTH: f64 = 0.42;
    const MIN_MARGIN_PIXELS: f64 = 12.0;
    (MARGIN_PER_BODY_LENGTH * body_length_pixels).max(MIN_MARGIN_PIXELS)
}

/// The corners of the box that holds every curve, including its half widths.
fn measure_curve_bounds(
    body_spine: &[CurvePoint],
    fin_curves: &[Vec<CurvePoint>],
) -> [[f64; 2]; 2] {
    let mut min_corner = [f64::INFINITY; 2];
    let mut max_corner = [f64::NEG_INFINITY; 2];
    for curve_point in body_spine.iter().chain(fin_curves.iter().flatten()) {
        for axis_index in 0..2 {
            min_corner[axis_index] = min_corner[axis_index]
                .min(curve_point.position[axis_index] - curve_point.half_width_pixels);
            max_corner[axis_index] = max_corner[axis_index]
                .max(curve_point.position[axis_index] + curve_point.half_width_pixels);
        }
    }
    [min_corner, max_corner]
}

fn sample_koi(
    body_spine: &[CurvePoint],
    fin_curves: &[Vec<CurvePoint>],
    sample_position: [f64; 2],
    pixel_position: [u32; 2],
    koi_appearance: &KoiAppearance,
) -> [u8; 4] {
    if let Some(body_sample) = sample_curve(body_spine, sample_position) {
        return shade_body(&body_sample, pixel_position, koi_appearance);
    }
    for fin_curve in fin_curves {
        if let Some(fin_sample) = sample_curve(fin_curve, sample_position) {
            return shade_fin(&fin_sample, pixel_position, koi_appearance.koi_coat);
        }
    }
    [0; 4]
}

// Constructed curves contain at least two points and no zero-length segments.
fn sample_curve(curve_points: &[CurvePoint], sample_position: [f64; 2]) -> Option<CurveSample> {
    let mut closest_sample = None;
    let mut closest_distance_pixels = f64::INFINITY;
    for (segment_index, end_points) in curve_points.windows(2).enumerate() {
        let [start_point, end_point] = [end_points[0], end_points[1]];
        let segment_x = end_point.position[0] - start_point.position[0];
        let segment_y = end_point.position[1] - start_point.position[1];
        let offset_x = sample_position[0] - start_point.position[0];
        let offset_y = sample_position[1] - start_point.position[1];
        let segment_length_pixels = segment_x.hypot(segment_y);
        let segment_progress = ((offset_x * segment_x + offset_y * segment_y)
            / segment_length_pixels.powi(2))
        .clamp(0.0, 1.0);
        let distance_pixels = (offset_x - segment_progress * segment_x)
            .hypot(offset_y - segment_progress * segment_y);
        if distance_pixels >= closest_distance_pixels {
            continue;
        }
        closest_distance_pixels = distance_pixels;
        closest_sample = Some(CurveSample {
            along_fraction: (segment_index as f64 + segment_progress)
                / (curve_points.len() - 1) as f64,
            lateral_pixels: (offset_x * segment_y - offset_y * segment_x) / segment_length_pixels,
            distance_pixels,
            half_width_pixels: start_point.half_width_pixels
                + segment_progress * (end_point.half_width_pixels - start_point.half_width_pixels),
            normal_direction: [
                segment_y / segment_length_pixels,
                -segment_x / segment_length_pixels,
            ],
        });
    }
    closest_sample
        .filter(|curve_sample| curve_sample.distance_pixels <= curve_sample.half_width_pixels)
}

fn shade_body(
    body_sample: &CurveSample,
    pixel_position: [u32; 2],
    koi_appearance: &KoiAppearance,
) -> [u8; 4] {
    let koi_anatomy = koi_appearance.koi_anatomy;
    let body_length_pixels = koi_anatomy.body_length_pixels;
    let lateral_fraction = body_sample.lateral_pixels / body_sample.half_width_pixels;
    let body_paint = koi_appearance.koi_coat.select_body_paint(
        body_sample.along_fraction,
        lateral_fraction,
        body_length_pixels,
        body_sample.half_width_pixels,
        calculate_dither_threshold(pixel_position),
    );
    const INNER_RIM_PIXELS: f64 = 0.8;
    if body_sample.distance_pixels > body_sample.half_width_pixels - INNER_RIM_PIXELS {
        return body_paint.shade_ramp[0];
    }
    let eye_lateral_fraction = 0.55;
    let eye_half_thickness_pixels = 0.8 * koi_anatomy.eye_scale;
    let eye_half_span_fraction = 0.25 * koi_anatomy.eye_scale.sqrt();
    if ((body_sample.along_fraction - koi_anatomy.eye_along_fraction) * body_length_pixels).abs()
        < eye_half_thickness_pixels
        && (lateral_fraction.abs() - eye_lateral_fraction).abs() < eye_half_span_fraction
    {
        return OUTLINE_COLOR;
    }
    if let Some(glint_color) = body_paint.glint_color {
        return glint_color;
    }
    let surface_brightness = calculate_body_brightness(body_sample, lateral_fraction, koi_anatomy)
        + body_paint.brightness_shift;
    adjust_saturation(
        choose_shade(body_paint.shade_ramp, surface_brightness, pixel_position),
        body_paint.saturation,
    )
}

fn calculate_body_brightness(
    body_sample: &CurveSample,
    lateral_fraction: f64,
    koi_anatomy: &KoiAnatomy,
) -> f64 {
    const AMBIENT_BRIGHTNESS: f64 = 0.24;
    const DIFFUSE_STRENGTH: f64 = 0.7;
    const LIGHT_DIRECTION: [f64; 3] = [-0.32, -0.5, 0.8];
    let surface_normal_z = (1.0 - lateral_fraction.powi(2)).max(0.0).sqrt();
    let incident_light = lateral_fraction
        * (body_sample.normal_direction[0] * LIGHT_DIRECTION[0]
            + body_sample.normal_direction[1] * LIGHT_DIRECTION[1])
        + surface_normal_z * LIGHT_DIRECTION[2];
    let mut surface_brightness = AMBIENT_BRIGHTNESS + DIFFUSE_STRENGTH * incident_light.max(0.0);
    let dorsal_span = 0.27..0.62;
    let dorsal_half_width_fraction = 0.1;
    let dorsal_shadow = 0.18;
    if dorsal_span.contains(&body_sample.along_fraction)
        && lateral_fraction.abs() < dorsal_half_width_fraction
    {
        surface_brightness -= dorsal_shadow;
    }
    // The gill line sits near the back of the head's slant.
    let gill_along_fraction = koi_anatomy.head_slant_fraction * 0.72;
    let gill_inner_fraction = 0.3;
    let gill_shadow = 0.22;
    if ((body_sample.along_fraction - gill_along_fraction) * koi_anatomy.body_length_pixels).abs()
        < 0.5
        && lateral_fraction.abs() > gill_inner_fraction
    {
        surface_brightness -= gill_shadow;
    }
    surface_brightness
}

fn shade_fin(fin_sample: &CurveSample, pixel_position: [u32; 2], koi_coat: &KoiCoat) -> [u8; 4] {
    const FIN_RIM_PIXELS: f64 = 0.6;
    const FIN_RAY_SPACING_PIXELS: f64 = 1.8;
    const FIN_ROOT_SHADOW: f64 = 0.12;
    let is_on_rim = fin_sample.distance_pixels > fin_sample.half_width_pixels - FIN_RIM_PIXELS;
    let fin_paint = koi_coat.select_fin_paint(
        fin_sample.along_fraction,
        fin_sample.lateral_pixels / fin_sample.half_width_pixels,
        is_on_rim,
        calculate_dither_threshold(pixel_position),
    );
    if is_on_rim {
        return adjust_saturation(fin_paint.shade_ramp[1], fin_paint.saturation);
    }
    let ray_phase = (fin_sample.lateral_pixels / FIN_RAY_SPACING_PIXELS).rem_euclid(1.0);
    let ray_brightness = 0.68;
    let membrane_brightness = 0.94;
    let surface_brightness = if ray_phase < 0.5 {
        ray_brightness
    } else {
        membrane_brightness
    };
    let root_shadow = FIN_ROOT_SHADOW * (1.0 - fin_sample.along_fraction).powi(2);
    adjust_saturation(
        choose_shade(
            fin_paint.shade_ramp,
            surface_brightness - root_shadow,
            pixel_position,
        ),
        fin_paint.saturation,
    )
}

/// The body is drawn as a chain of round segments. The head widens from a round nose
/// along a gentle slant, so no segment outgrows the one in front of it: a steeper rise
/// leaves a knob at the tip, and a narrow head behind a wide body leaves a pinch.
fn calculate_body_half_width(along_fraction: f64, koi_anatomy: &KoiAnatomy) -> f64 {
    const MIN_SHOULDER_FRACTION: f64 = 0.3;
    const SHOULDER_GAP_FRACTION: f64 = 0.04;
    const TAPER_END_FRACTION: f64 = 0.92;
    const TAIL_WIDTH_FRACTION: f64 = 0.26;
    let head_slant_fraction = koi_anatomy.head_slant_fraction;
    let width_fraction = if along_fraction < head_slant_fraction {
        let nose_width_fraction = koi_anatomy.nose_width_fraction;
        nose_width_fraction
            + (1.0 - nose_width_fraction) * (along_fraction / head_slant_fraction * PI / 2.0).sin()
    } else {
        let shoulder_fraction =
            (head_slant_fraction + SHOULDER_GAP_FRACTION).max(MIN_SHOULDER_FRACTION);
        let taper_length_fraction = TAPER_END_FRACTION - shoulder_fraction;
        (1.0 - ((along_fraction - shoulder_fraction).max(0.0) / taper_length_fraction)
            .powf(koi_anatomy.tail_taper_exponent))
        .max(TAIL_WIDTH_FRACTION)
    };
    koi_anatomy.body_width_pixels / 2.0 * width_fraction
}

fn calculate_forward_direction(curve_points: &[CurvePoint], point_index: usize) -> [f64; 2] {
    const DIRECTION_SPAN_SAMPLES: usize = 2;
    let earlier_point = curve_points[point_index.saturating_sub(DIRECTION_SPAN_SAMPLES)].position;
    let later_point =
        curve_points[(point_index + DIRECTION_SPAN_SAMPLES).min(curve_points.len() - 1)].position;
    let tangent_x = earlier_point[0] - later_point[0];
    let tangent_y = earlier_point[1] - later_point[1];
    let tangent_length_pixels = tangent_x.hypot(tangent_y);
    [
        tangent_x / tangent_length_pixels,
        tangent_y / tangent_length_pixels,
    ]
}

fn paint_outline(rgba_pixels: &mut [[u8; 4]]) {
    let original_pixels = rgba_pixels.to_vec();
    let frame_width = FRAME_WIDTH_PIXELS as usize;
    for (pixel_index, pixel_channels) in rgba_pixels.iter_mut().enumerate() {
        if original_pixels[pixel_index][3] != 0 {
            continue;
        }
        let pixel_x = pixel_index % frame_width;
        let pixel_y = pixel_index / frame_width;
        let touches_shape = (pixel_x > 0 && original_pixels[pixel_index - 1][3] != 0)
            || (pixel_x + 1 < frame_width && original_pixels[pixel_index + 1][3] != 0)
            || (pixel_y > 0 && original_pixels[pixel_index - frame_width][3] != 0)
            || (pixel_y + 1 < FRAME_HEIGHT_PIXELS as usize
                && original_pixels[pixel_index + frame_width][3] != 0);
        if touches_shape {
            *pixel_channels = OUTLINE_COLOR;
        }
    }
}

fn choose_shade(
    shade_ramp: &[[u8; 4]],
    surface_brightness: f64,
    pixel_position: [u32; 2],
) -> [u8; 4] {
    let shade_position = surface_brightness.clamp(0.0, 1.0) * (shade_ramp.len() - 1) as f64;
    let base_index = shade_position.floor() as usize;
    let shade_index = base_index
        + usize::from(shade_position.fract() > calculate_dither_threshold(pixel_position));
    shade_ramp[shade_index.min(shade_ramp.len() - 1)]
}

/// Scale a color's distance from its own gray; one leaves it unchanged.
fn adjust_saturation(pixel_color: [u8; 4], saturation: f64) -> [u8; 4] {
    let [red, green, blue, alpha] = pixel_color.map(f64::from);
    let gray = 0.3 * red + 0.59 * green + 0.11 * blue;
    let saturate = |channel: f64| {
        (gray + (channel - gray) * saturation)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    [saturate(red), saturate(green), saturate(blue), alpha as u8]
}

/// The ordered-dither threshold at a pixel, from zero through one.
fn calculate_dither_threshold(pixel_position: [u32; 2]) -> f64 {
    let dither_x = pixel_position[0] as usize % DITHER_RANKS[0].len();
    let dither_y = pixel_position[1] as usize % DITHER_RANKS.len();
    (f64::from(DITHER_RANKS[dither_y][dither_x]) + 0.5)
        / (DITHER_RANKS.len() * DITHER_RANKS[0].len()) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Paint only the mouth on an empty canvas centered on it.
    fn paint_mouth_alone(koi_body: KoiBody, mouth_opening: f64) -> Vec<[u8; 4]> {
        let koi_anatomy = calculate_anatomy(1_500);
        let body_spine = build_spine(&koi_anatomy, &STILL_POSE);
        let mut rgba_pixels = vec![[0; 4]; (FRAME_WIDTH_PIXELS * FRAME_HEIGHT_PIXELS) as usize];
        let mouth_position = calculate_mouth_position(&body_spine);
        paint_mouth(
            &mut rgba_pixels,
            &body_spine,
            koi_body,
            mouth_position,
            mouth_opening,
        );
        rgba_pixels
    }

    fn center_pixel(rgba_pixels: &[[u8; 4]]) -> [u8; 4] {
        rgba_pixels
            [(FRAME_HEIGHT_PIXELS / 2 * FRAME_WIDTH_PIXELS + FRAME_WIDTH_PIXELS / 2) as usize]
    }

    fn count_painted(rgba_pixels: &[[u8; 4]]) -> usize {
        rgba_pixels
            .iter()
            .filter(|pixel_channels| pixel_channels[3] != 0)
            .count()
    }

    #[test]
    fn the_mouth_opens_as_a_body_colored_rim_around_a_dark_opening() {
        assert_eq!(count_painted(&paint_mouth_alone(KoiBody::White, 0.1)), 0);
        for (koi_body, rim_color) in [
            (KoiBody::White, [192, 203, 220, 255]),
            (KoiBody::Black, [58, 68, 102, 255]),
        ] {
            let open_mouth = paint_mouth_alone(koi_body, 1.0);
            assert_eq!(center_pixel(&open_mouth), OUTLINE_COLOR);
            assert!(open_mouth.contains(&rim_color), "{koi_body:?}");
            assert!(
                open_mouth
                    .iter()
                    .all(|&pixel| pixel == [0; 4] || pixel == OUTLINE_COLOR || pixel == rim_color)
            );
        }
        let half_open = count_painted(&paint_mouth_alone(KoiBody::White, 0.5));
        let wide_open = count_painted(&paint_mouth_alone(KoiBody::White, 1.0));
        assert!(0 < half_open && half_open < wide_open);
    }
}
