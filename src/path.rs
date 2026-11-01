//! The closed course a swimming koi follows: a smooth, irregular loop that wanders the
//! full width of the frame and now and then swoops toward its top or bottom edge.
//!
//! The course passes through fixed waypoints on a centripetal Catmull-Rom curve, is
//! smoothed so its bend changes gradually, and is resampled so neighboring points sit
//! one pixel of arc apart. Distances wrap around the loop. Each koi swims the course
//! fitted to its own size, so larger koi keep a wider margin from the frame edges.

use std::f64::consts::{PI, TAU};
use std::sync::OnceLock;

use crate::render::{FRAME_HEIGHT_PIXELS, FRAME_WIDTH_PIXELS};

/// Room between the course and the frame edges that the course's shape is designed at.
const DESIGN_EDGE_MARGIN_PIXELS: [f64; 2] = [44.0, 40.0];
/// Waypoints in shares of the reachable half width and half height, in swimming order.
/// The koi crosses left to right through the middle, turns up at the right edge, dives
/// to the bottom on the way back, and turns again at the left edge.
const WAYPOINTS: [[f64; 2]; 15] = [
    [-0.6, 0.6],
    [-0.28, 0.02],
    [0.04, 0.38],
    [0.36, -0.08],
    [0.66, 0.5],
    [0.95, 0.42],
    [0.98, -0.5],
    [0.72, -0.9],
    [0.42, -0.6],
    [0.12, 0.2],
    [-0.18, 0.9],
    [-0.48, 0.25],
    [-0.66, -0.78],
    [-0.94, -0.6],
    [-0.96, 0.3],
];
const SAMPLES_PER_SEGMENT: usize = 200;
const SMOOTHING_PASSES: usize = 80;
const SMOOTHING_REACH_PIXELS: usize = 6;
/// The bend is measured over this much arc on each side of a point.
const CURVATURE_REACH_PIXELS: f64 = 8.0;

/// The course as points one pixel of arc apart, centered on the frame center.
pub(crate) struct SwimPath {
    points: Vec<[f64; 2]>,
}

/// The course at its design size.
pub(crate) fn swim_path() -> &'static SwimPath {
    static SWIM_PATH: OnceLock<SwimPath> = OnceLock::new();
    SWIM_PATH.get_or_init(SwimPath::build)
}

impl SwimPath {
    fn build() -> Self {
        let reach = calculate_reach(DESIGN_EDGE_MARGIN_PIXELS);
        let waypoints: Vec<[f64; 2]> = WAYPOINTS
            .iter()
            .map(|waypoint| [waypoint[0] * reach[0], waypoint[1] * reach[1]])
            .collect();
        let mut dense_points = sample_catmull_rom(&waypoints);
        for _ in 0..SMOOTHING_PASSES {
            dense_points = resample_by_arc(&dense_points, 1.0);
            dense_points = smooth(&dense_points, SMOOTHING_REACH_PIXELS);
        }
        let mut points = resample_by_arc(&dense_points, 1.0);
        // Smoothing pulls the loop inward; stretch it back out to the reachable box.
        for axis_index in 0..2 {
            let (min, max) = points
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), point| {
                    (min.min(point[axis_index]), max.max(point[axis_index]))
                });
            let center = (min + max) / 2.0;
            let scale = 2.0 * reach[axis_index] / (max - min);
            for point in &mut points {
                point[axis_index] = (point[axis_index] - center) * scale;
            }
        }
        Self {
            points: resample_by_arc(&points, 1.0),
        }
    }

    /// The same course stretched or shrunk on each axis to keep this margin from every
    /// frame edge.
    pub(crate) fn fit_to_margin(&self, edge_margin_pixels: f64) -> Self {
        let design_reach = calculate_reach(DESIGN_EDGE_MARGIN_PIXELS);
        let reach = calculate_reach([edge_margin_pixels; 2]);
        let points: Vec<[f64; 2]> = self
            .points
            .iter()
            .map(|point| {
                [
                    point[0] * reach[0] / design_reach[0],
                    point[1] * reach[1] / design_reach[1],
                ]
            })
            .collect();
        Self {
            points: resample_by_arc(&points, 1.0),
        }
    }

    /// The loop's length in pixels of arc.
    pub(crate) fn length_pixels(&self) -> f64 {
        self.points.len() as f64
    }

    /// Where the course is after this much arc from its start, wrapping around the loop.
    pub(crate) fn position_at(&self, distance_pixels: f64) -> [f64; 2] {
        let point_count = self.points.len();
        let wrapped = distance_pixels.rem_euclid(point_count as f64);
        let index = wrapped.floor() as usize % point_count;
        let next = self.points[(index + 1) % point_count];
        let point = self.points[index];
        let fraction = wrapped.fract();
        [
            point[0] + (next[0] - point[0]) * fraction,
            point[1] + (next[1] - point[1]) * fraction,
        ]
    }

    /// The course's bend at a distance, in radians per pixel: positive where it turns
    /// clockwise on screen, negative where it turns counterclockwise.
    pub(crate) fn curvature_at(&self, distance_pixels: f64) -> f64 {
        let heading = |from: f64, to: f64| {
            let [from_x, from_y] = self.position_at(from);
            let [to_x, to_y] = self.position_at(to);
            (to_y - from_y).atan2(to_x - from_x)
        };
        let behind = heading(distance_pixels - CURVATURE_REACH_PIXELS, distance_pixels);
        let ahead = heading(distance_pixels, distance_pixels + CURVATURE_REACH_PIXELS);
        ((ahead - behind + PI).rem_euclid(TAU) - PI) / CURVATURE_REACH_PIXELS
    }
}

/// How far the course may reach from the frame center on each axis.
fn calculate_reach(edge_margin_pixels: [f64; 2]) -> [f64; 2] {
    [
        f64::from(FRAME_WIDTH_PIXELS) / 2.0 - edge_margin_pixels[0],
        f64::from(FRAME_HEIGHT_PIXELS) / 2.0 - edge_margin_pixels[1],
    ]
}

/// A closed centripetal Catmull-Rom curve through the waypoints.
fn sample_catmull_rom(waypoints: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let waypoint_count = waypoints.len();
    let knot_gap =
        |from: [f64; 2], to: [f64; 2]| ((to[0] - from[0]).hypot(to[1] - from[1])).sqrt().max(1e-6);
    let mut samples = Vec::with_capacity(waypoint_count * SAMPLES_PER_SEGMENT);
    for segment_index in 0..waypoint_count {
        let p0 = waypoints[(segment_index + waypoint_count - 1) % waypoint_count];
        let p1 = waypoints[segment_index];
        let p2 = waypoints[(segment_index + 1) % waypoint_count];
        let p3 = waypoints[(segment_index + 2) % waypoint_count];
        let t1 = knot_gap(p0, p1);
        let t2 = t1 + knot_gap(p1, p2);
        let t3 = t2 + knot_gap(p2, p3);
        let lerp = |a: [f64; 2], b: [f64; 2], ta: f64, tb: f64, t: f64| {
            let w = (t - ta) / (tb - ta);
            [a[0] + (b[0] - a[0]) * w, a[1] + (b[1] - a[1]) * w]
        };
        for sample_index in 0..SAMPLES_PER_SEGMENT {
            let t = t1 + (t2 - t1) * sample_index as f64 / SAMPLES_PER_SEGMENT as f64;
            let a1 = lerp(p0, p1, 0.0, t1, t);
            let a2 = lerp(p1, p2, t1, t2, t);
            let a3 = lerp(p2, p3, t2, t3, t);
            let b1 = lerp(a1, a2, 0.0, t2, t);
            let b2 = lerp(a2, a3, t1, t3, t);
            samples.push(lerp(b1, b2, t1, t2, t));
        }
    }
    samples
}

/// Points along a closed polyline, `spacing_pixels` of arc apart, with the spacing
/// stretched slightly so the loop closes on a whole number of steps.
fn resample_by_arc(points: &[[f64; 2]], spacing_pixels: f64) -> Vec<[f64; 2]> {
    let point_count = points.len();
    let segment_lengths: Vec<f64> = (0..point_count)
        .map(|index| {
            let [x0, y0] = points[index];
            let [x1, y1] = points[(index + 1) % point_count];
            (x1 - x0).hypot(y1 - y0)
        })
        .collect();
    let total_length: f64 = segment_lengths.iter().sum();
    let step_count = (total_length / spacing_pixels).round().max(3.0) as usize;
    let step = total_length / step_count as f64;
    let mut resampled = Vec::with_capacity(step_count);
    let mut segment_index = 0;
    let mut segment_start = 0.0;
    for step_index in 0..step_count {
        let target = step_index as f64 * step;
        while segment_index + 1 < point_count
            && segment_start + segment_lengths[segment_index] < target
        {
            segment_start += segment_lengths[segment_index];
            segment_index += 1;
        }
        let fraction = if segment_lengths[segment_index] > 0.0 {
            (target - segment_start) / segment_lengths[segment_index]
        } else {
            0.0
        };
        let [x0, y0] = points[segment_index];
        let [x1, y1] = points[(segment_index + 1) % point_count];
        resampled.push([x0 + (x1 - x0) * fraction, y0 + (y1 - y0) * fraction]);
    }
    resampled
}

/// A moving average over the neighbors within reach on each side of every point.
fn smooth(points: &[[f64; 2]], reach: usize) -> Vec<[f64; 2]> {
    let point_count = points.len();
    let window = (2 * reach + 1) as f64;
    (0..point_count)
        .map(|index| {
            let mut sum = [0.0; 2];
            for offset in 0..=2 * reach {
                let point = points[(index + point_count + offset - reach) % point_count];
                sum[0] += point[0];
                sum[1] += point[1];
            }
            [sum[0] / window, sum[1] / window]
        })
        .collect()
}
