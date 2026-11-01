use std::f64::consts::TAU;

use git_koi::behavior::Behavior;
use git_koi::motion::{Course, SwimPose, calculate_swim_pose, frame_count};

/// A pose value's name, how to read it, and whether it wraps around a full turn and
/// so is compared around the circle.
type PoseValue = (&'static str, fn(&SwimPose) -> f64, bool);

const POSE_VALUES: [PoseValue; 17] = [
    ("lap angle", read_lap_angle, true),
    ("wave phase", |pose| pose.wave_phase_radians, true),
    ("wave strength", |pose| pose.wave_strength, false),
    (
        "left tail lobe",
        |pose| pose.tail_lobe_bends_radians[0],
        false,
    ),
    (
        "right tail lobe",
        |pose| pose.tail_lobe_bends_radians[1],
        false,
    ),
    ("tail spread", |pose| pose.tail_spread_radians, false),
    (
        "left side fin",
        |pose| pose.side_fin_openings_radians[0],
        false,
    ),
    (
        "right side fin",
        |pose| pose.side_fin_openings_radians[1],
        false,
    ),
    (
        "left side fin bend",
        |pose| pose.side_fin_bends_radians[0],
        false,
    ),
    ("pelvic fins", |pose| pose.pelvic_fin_opening_radians, false),
    ("whiskers", |pose| pose.whisker_sway_radians, false),
    ("heading offset", |pose| read_hover(pose).0, false),
    ("drift across", |pose| read_hover(pose).1[0], false),
    ("drift down", |pose| read_hover(pose).1[1], false),
    ("depth", |pose| pose.depth, false),
    // A cycle from zero up to one, compared around the circle like an angle.
    (
        "feeding phase",
        |pose| pose.feeding_phase.unwrap_or(0.0) * TAU,
        true,
    ),
    ("mouth opening", |pose| pose.mouth_opening, false),
];

/// The largest step each pose value may take between frames, in POSE_VALUES order.
const STEP_LIMITS: [(Behavior, [f64; 17]); 5] = [
    (
        Behavior::Cruise,
        [
            0.08,
            TAU / 8.0 + 1e-9,
            0.1,
            0.55,
            0.55,
            0.15,
            0.07,
            0.07,
            0.12,
            0.03,
            0.13,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.2,
        ],
    ),
    (
        Behavior::Dart,
        [
            0.33,
            TAU / 6.0 + 1e-9,
            0.42,
            1.05,
            1.05,
            0.2,
            0.11,
            0.11,
            0.22,
            0.035,
            0.16,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.2,
        ],
    ),
    (
        Behavior::Rest,
        [
            0.0,
            TAU / 15.0 + 1e-9,
            0.1,
            0.15,
            0.15,
            0.0,
            0.06,
            0.06,
            0.12,
            0.01,
            0.07,
            0.01,
            0.25,
            0.3,
            0.0,
            0.0,
            0.22,
        ],
    ),
    (
        Behavior::DeepRest,
        [
            0.0,
            TAU / 30.0 + 1e-9,
            0.02,
            0.05,
            0.05,
            0.0,
            0.14,
            0.14,
            0.3,
            1e-9,
            0.04,
            0.005,
            0.1,
            0.12,
            0.0,
            0.0,
            0.2,
        ],
    ),
    (
        Behavior::SurfaceFeeding,
        [
            0.0,
            TAU / 15.0 + 1e-9,
            0.1,
            0.15,
            0.15,
            0.0,
            0.06,
            0.06,
            0.12,
            0.01,
            0.07,
            0.01,
            0.25,
            0.3,
            0.0,
            TAU * 3.0 / 90.0 + 1e-9,
            0.22,
        ],
    ),
];

#[test]
fn every_pose_value_flows_across_the_loop_boundary() {
    for (behavior, step_limits) in STEP_LIMITS {
        assert_pose_values_flow(behavior, step_limits);
    }
}

#[test]
fn strokes_waves_and_sculls_keep_whole_rhythms() {
    let poses = collect_loop(Behavior::Cruise);
    for stroke_half in poses.chunks(60) {
        let strongest_wave = stroke_half
            .iter()
            .map(|pose| pose.wave_strength)
            .fold(0.0, f64::max);
        assert!(strongest_wave > 1.15 && strongest_wave <= 1.2);
        assert!(
            stroke_half[40..]
                .iter()
                .all(|pose| (pose.wave_strength - 0.3).abs() < 1e-12)
        );
    }
    for frame_index in 0..frame_count(Behavior::Cruise) {
        let pose = cruise_pose(frame_index);
        let wave_later = cruise_pose(frame_index + 8);
        let phase_step = (wave_later.wave_phase_radians - pose.wave_phase_radians).rem_euclid(TAU);
        assert!(phase_step < 1e-9 || TAU - phase_step < 1e-9);
    }
    // The inside flare follows the course's turns, so only the loop holds whole sculls.
    assert_eq!(frame_count(Behavior::Cruise) % 20, 0);
}

#[test]
fn tail_lobes_trail_out_of_step() {
    let lobe_differences: Vec<f64> = collect_loop(Behavior::Cruise)
        .iter()
        .map(|pose| (pose.tail_lobe_bends_radians[1] - pose.tail_lobe_bends_radians[0]).abs())
        .collect();
    let average_difference = lobe_differences.iter().sum::<f64>() / lobe_differences.len() as f64;
    assert!(average_difference > 0.05);
}

#[test]
fn side_fin_tips_trail_their_roots() {
    let cruise_frame_count = frame_count(Behavior::Cruise);
    let mut largest_bend: f64 = 0.0;
    for frame_index in 0..cruise_frame_count {
        let pose = cruise_pose(frame_index);
        let earlier_pose = cruise_pose(frame_index + cruise_frame_count - 3);
        for side_index in 0..2 {
            let expected_bend = earlier_pose.side_fin_openings_radians[side_index]
                - pose.side_fin_openings_radians[side_index];
            assert!((pose.side_fin_bends_radians[side_index] - expected_bend).abs() < 1e-12);
            largest_bend = largest_bend.max(pose.side_fin_bends_radians[side_index].abs());
        }
    }
    assert!(largest_bend > 0.05);
}

#[test]
fn the_inside_fin_flares_and_both_brake_on_the_glide() {
    let poses = collect_loop(Behavior::Cruise);
    let average_flare = poses
        .iter()
        .map(|pose| pose.side_fin_openings_radians[1] - pose.side_fin_openings_radians[0])
        .sum::<f64>()
        / poses.len() as f64;
    // The course turns both ways, so each fin is inside some turns.
    assert!(average_flare.abs() < 0.24);
    let flares = poses
        .iter()
        .map(|pose| pose.side_fin_openings_radians[1] - pose.side_fin_openings_radians[0]);
    assert!(flares.clone().any(|flare| flare > 0.2));
    assert!(flares.clone().any(|flare| flare < -0.2));
    assert!(average_shared_opening(&poses[40..60]) > average_shared_opening(&poses[10..25]) + 0.2);
}

#[test]
fn darting_swims_one_lap_of_bursts_with_tucked_then_spread_fins() {
    let poses = collect_loop(Behavior::Dart);
    let lap_turn_radians: f64 = poses
        .iter()
        .zip(poses.iter().cycle().skip(1))
        .map(|(pose, next_pose)| {
            (read_lap_angle(next_pose) - read_lap_angle(pose) + TAU / 2.0).rem_euclid(TAU)
                - TAU / 2.0
        })
        .sum();
    assert!((lap_turn_radians - TAU).abs() < 1e-9);
    for (frame_index, pose) in poses.iter().enumerate() {
        let burst_later = calculate_swim_pose(Behavior::Dart, frame_index as u32 + 30);
        assert!((burst_later.wave_strength - pose.wave_strength).abs() < 1e-12);
    }
    let strongest_wave = poses
        .iter()
        .map(|pose| pose.wave_strength)
        .fold(0.0, f64::max);
    assert!(strongest_wave > 1.6);
    // Frame 5 is mid-burst and frame 20 is mid-glide.
    assert!(average_shared_opening(&poses[5..6]) < -0.1);
    assert!(average_shared_opening(&poses[20..21]) > 0.35);
    assert!(poses[5].pelvic_fin_opening_radians < 0.0);
}

#[test]
fn resting_hovers_a_little_deeper_and_deep_rest_lower_still() {
    for (behavior, expected_frame_count, expected_depth) in
        [(Behavior::Rest, 90, 0.22), (Behavior::DeepRest, 120, 0.5)]
    {
        assert_eq!(frame_count(behavior), expected_frame_count);
        for pose in collect_loop(behavior) {
            assert!(matches!(pose.course, Course::Hover { .. }));
            assert_eq!(pose.depth, expected_depth);
        }
    }
    let average_drift_down = |behavior| {
        let poses = collect_loop(behavior);
        poses.iter().map(|pose| read_hover(pose).1[1]).sum::<f64>() / poses.len() as f64
    };
    assert!(average_drift_down(Behavior::DeepRest) > average_drift_down(Behavior::Rest) + 3.0);
    for behavior in [Behavior::Cruise, Behavior::Dart] {
        assert!(
            collect_loop(behavior)
                .iter()
                .all(|pose| matches!(pose.course, Course::Lap { .. }) && pose.depth == 0.0)
        );
    }
}

#[test]
fn deep_rest_flicks_one_fin_once_per_loop() {
    let flicking_frames: Vec<usize> = collect_loop(Behavior::DeepRest)
        .iter()
        .enumerate()
        .filter(|(_, pose)| {
            pose.side_fin_openings_radians[1] - pose.side_fin_openings_radians[0] > 0.01
        })
        .map(|(frame_index, _)| frame_index)
        .collect();
    assert!(!flicking_frames.is_empty() && flicking_frames.len() < 10);
    assert!(
        flicking_frames
            .windows(2)
            .all(|frame_pair| frame_pair[1] == frame_pair[0] + 1)
    );
}

#[test]
fn surface_feeding_rests_at_the_surface_with_ripples_on_its_own_beat() {
    assert_eq!(
        frame_count(Behavior::SurfaceFeeding),
        frame_count(Behavior::Rest)
    );
    let feeding_poses = collect_loop(Behavior::SurfaceFeeding);
    for (feeding_pose, rest_pose) in feeding_poses.iter().zip(collect_loop(Behavior::Rest)) {
        assert_eq!(feeding_pose.depth, 0.0);
        assert_eq!(
            SwimPose {
                depth: rest_pose.depth,
                feeding_phase: None,
                ..*feeding_pose
            },
            rest_pose
        );
    }
    let feeding_phases: Vec<f64> = feeding_poses
        .iter()
        .map(|pose| pose.feeding_phase.unwrap())
        .collect();
    let cycle_starts = feeding_phases
        .iter()
        .zip(feeding_phases.iter().cycle().skip(1))
        .filter(|(phase, next_phase)| next_phase < phase)
        .count();
    assert_eq!(cycle_starts, 3);
    for behavior in [
        Behavior::Cruise,
        Behavior::Dart,
        Behavior::Rest,
        Behavior::DeepRest,
    ] {
        assert!(
            collect_loop(behavior)
                .iter()
                .all(|pose| pose.feeding_phase.is_none())
        );
    }
}

#[test]
fn the_mouth_breathes_while_lapping_and_gulps_while_resting() {
    // Separate openings around the loop, and the widest opening.
    let measure = |behavior| {
        let openings: Vec<f64> = collect_loop(behavior)
            .iter()
            .map(|pose| pose.mouth_opening)
            .collect();
        let opening_count = openings
            .iter()
            .zip(openings.iter().cycle().skip(1))
            .filter(|(opening, next_opening)| **opening <= 0.1 && **next_opening > 0.1)
            .count();
        (opening_count, openings.iter().copied().fold(0.0, f64::max))
    };
    for behavior in [Behavior::Cruise, Behavior::Dart] {
        let (breath_count, widest) = measure(behavior);
        // One breath every 24 frames.
        assert_eq!(
            breath_count as u32,
            frame_count(behavior) / 24,
            "{behavior:?}"
        );
        assert!((widest - 0.7).abs() < 1e-9, "{behavior:?}");
    }
    for behavior in [Behavior::Rest, Behavior::SurfaceFeeding] {
        let (gulp_count, widest) = measure(behavior);
        assert_eq!(gulp_count, 3, "{behavior:?}");
        assert!(widest > 0.99, "{behavior:?}");
    }
    let deep_rest_poses = collect_loop(Behavior::DeepRest);
    let open_frames: Vec<usize> = (0..deep_rest_poses.len())
        .filter(|&frame_index| deep_rest_poses[frame_index].mouth_opening > 0.0)
        .collect();
    assert_eq!(measure(Behavior::DeepRest).0, 1);
    assert!(
        open_frames
            .iter()
            .all(|frame_index| (66..=82).contains(frame_index))
    );
    assert!(open_frames.contains(&70) && open_frames.contains(&78));
}

fn assert_pose_values_flow(behavior: Behavior, step_limits: [f64; 17]) {
    let loop_frame_count = frame_count(behavior) as usize;
    let mut poses = collect_loop(behavior);
    poses.push(calculate_swim_pose(behavior, loop_frame_count as u32));
    assert_eq!(poses[loop_frame_count], poses[0]);
    for ((value_name, read_value, wraps_around), largest_step) in
        POSE_VALUES.into_iter().zip(step_limits)
    {
        let steps: Vec<f64> = poses
            .windows(2)
            .map(|pose_pair| {
                let step = read_value(&pose_pair[1]) - read_value(&pose_pair[0]);
                if wraps_around {
                    (step + TAU / 2.0).rem_euclid(TAU) - TAU / 2.0
                } else {
                    step
                }
                .abs()
            })
            .collect();
        let boundary_step = steps[loop_frame_count - 1];
        let regular_largest = steps[..loop_frame_count - 1]
            .iter()
            .copied()
            .fold(0.0, f64::max);
        assert!(
            boundary_step <= regular_largest + 1e-9,
            "{behavior:?} {value_name} jumps across the loop boundary"
        );
        assert!(
            regular_largest <= largest_step + 1e-12,
            "{behavior:?} {value_name} jumps by {regular_largest}"
        );
    }
}

fn collect_loop(behavior: Behavior) -> Vec<SwimPose> {
    (0..frame_count(behavior))
        .map(|frame_index| calculate_swim_pose(behavior, frame_index))
        .collect()
}

fn read_lap_angle(pose: &SwimPose) -> f64 {
    match pose.course {
        Course::Lap { course_fraction } => TAU * course_fraction,
        Course::Hover { .. } => 0.0,
    }
}

/// The heading offset and drift of a hovering pose; laps neither turn nor drift.
fn read_hover(pose: &SwimPose) -> (f64, [f64; 2]) {
    match pose.course {
        Course::Hover {
            heading_offset_radians,
            drift_pixels,
        } => (heading_offset_radians, drift_pixels),
        Course::Lap { .. } => (0.0, [0.0; 2]),
    }
}

fn cruise_pose(frame_index: u32) -> SwimPose {
    calculate_swim_pose(Behavior::Cruise, frame_index)
}

/// The opening both side fins share, without the inside flare and scull.
fn average_shared_opening(poses: &[SwimPose]) -> f64 {
    poses
        .iter()
        .map(|pose| pose.side_fin_openings_radians.iter().sum::<f64>() / 2.0)
        .sum::<f64>()
        / poses.len() as f64
}
