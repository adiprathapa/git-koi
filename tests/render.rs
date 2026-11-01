use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;

use chrono::{Datelike, Days, NaiveDate};
use git_koi::behavior::Behavior;
use git_koi::coat::KoiBody;
use git_koi::history::{AccountSnapshot, ContributionDay, ContributionWeek, LanguageColor};
use git_koi::koi::{
    ADULT_CONTRIBUTIONS, FULL_COLOR_CONTRIBUTIONS, KoiDevelopment, MAX_GROWTH_CONTRIBUTIONS,
    RecentStreak, derive_koi_development,
};
use git_koi::motion::frame_count;
use git_koi::output::encode_png;
use git_koi::render::{
    FRAME_HEIGHT_PIXELS, FRAME_WIDTH_PIXELS, KoiFrame, render_koi, render_swim_frame,
};

type LanguageMix<'a> = &'a [(&'a str, Option<&'a str>, f64)];

const RED_MIX: LanguageMix = &[("C++", Some("#f34b7d"), 1.0)];
const BODIES: [KoiBody; 7] = [
    KoiBody::White,
    KoiBody::Black,
    KoiBody::Asagi,
    KoiBody::Tea,
    KoiBody::Gold,
    KoiBody::Platinum,
    KoiBody::Blue,
];
const FOUR_LANGUAGE_MIX: LanguageMix = &[
    ("C", Some("#555555"), 0.25),
    ("Python", Some("#3572a5"), 0.25),
    ("C++", Some("#f34b7d"), 0.25),
    ("JavaScript", Some("#f1e05a"), 0.25),
];

#[test]
fn language_mixes_color_the_same_fish_reproducibly() {
    let uncolored_frame = render_koi(&develop_with_languages(1_500, &[]), KoiBody::default());
    let mut distinct_images = BTreeSet::new();
    for language_mix in [
        RED_MIX,
        &[
            ("Python", Some("#3572a5"), 0.65),
            ("C", Some("#555555"), 0.35),
        ],
        &[
            ("TypeScript", Some("#3178c6"), 0.45),
            ("C++", Some("#f34b7d"), 0.35),
            ("Shell", Some("#89e051"), 0.2),
        ],
        FOUR_LANGUAGE_MIX,
        &[("Rust", Some("#dea584"), 1.0)],
    ] {
        let koi_frame = render_koi(
            &develop_with_languages(1_500, language_mix),
            KoiBody::default(),
        );
        assert_ne!(koi_frame, uncolored_frame);
        for (uncolored_pixel, colored_pixel) in uncolored_frame
            .as_rgba_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(koi_frame.as_rgba_bytes().as_chunks::<4>().0.iter())
        {
            assert_eq!(uncolored_pixel[3], colored_pixel[3]);
        }
        let png_bytes = encode_png(&koi_frame).unwrap();
        assert_eq!(
            png_bytes,
            encode_png(&render_koi(
                &develop_with_languages(1_500, language_mix),
                KoiBody::default()
            ))
            .unwrap()
        );
        assert!(distinct_images.insert(png_bytes));
    }
}

#[test]
fn languages_on_one_pigment_share_its_patches() {
    let render_mix = |language_mix| {
        render_koi(
            &develop_with_languages(ADULT_CONTRIBUTIONS, language_mix),
            KoiBody::default(),
        )
    };
    // Python and TypeScript are both blue; C's gray and a missing color are both black.
    assert_eq!(
        render_mix(&[
            ("Python", Some("#3572a5"), 0.5),
            ("TypeScript", Some("#3178c6"), 0.5),
        ]),
        render_mix(&[("Python", Some("#3572a5"), 1.0)])
    );
    assert_eq!(
        render_mix(&[("C", Some("#555555"), 1.0)]),
        render_mix(&[("Unrecognized", None, 1.0)])
    );
    assert_ne!(
        render_mix(&[
            ("Python", Some("#3572a5"), 0.5),
            ("Rust", Some("#dea584"), 0.5),
        ]),
        render_mix(&[("Python", Some("#3572a5"), 1.0)])
    );
}

#[test]
fn equal_pigment_shares_tie_despite_rounding() {
    let render_mix = |language_mix| {
        render_koi(
            &develop_with_languages(ADULT_CONTRIBUTIONS, language_mix),
            KoiBody::default(),
        )
    };
    // Orange sums to 0.30000000000000004 here; it still ties with yellow, so yellow
    // keeps its place and gives the blue koi its sides.
    assert_eq!(
        render_mix(&[
            ("Python", Some("#3572a5"), 0.4),
            ("Rust", Some("#dea584"), 0.1),
            ("Java", Some("#b07219"), 0.2),
            ("JavaScript", Some("#f1e05a"), 0.3),
        ]),
        render_mix(&[
            ("Python", Some("#3572a5"), 0.4),
            ("Java", Some("#b07219"), 0.3),
            ("JavaScript", Some("#f1e05a"), 0.3),
        ])
    );
}

#[test]
fn the_same_languages_form_different_patches_on_another_account() {
    let mut koi_development = develop_with_languages(ADULT_CONTRIBUTIONS, FOUR_LANGUAGE_MIX);
    let original_frame = render_koi(&koi_development, KoiBody::default());
    koi_development.identity_seed += 1_u64 << 32;
    assert_ne!(
        original_frame,
        render_koi(&koi_development, KoiBody::default())
    );
}

#[test]
fn young_koi_show_pale_pigments_until_full_color() {
    let collect_mix_colors = |contribution_total, language_mix| {
        collect_opaque_colors(&render_koi(
            &develop_with_languages(contribution_total, language_mix),
            KoiBody::default(),
        ))
    };
    let full_colors = collect_mix_colors(MAX_GROWTH_CONTRIBUTIONS, RED_MIX);
    let red_colors: BTreeSet<_> = full_colors
        .difference(&collect_mix_colors(MAX_GROWTH_CONTRIBUTIONS, &[]))
        .copied()
        .collect();
    let pale_colors: BTreeSet<_> = collect_mix_colors(0, RED_MIX)
        .difference(&collect_mix_colors(0, &[]))
        .filter(|pixel_color| !full_colors.contains(*pixel_color))
        .copied()
        .collect();
    assert!(!pale_colors.is_empty());
    let midway_colors = collect_mix_colors(100, RED_MIX);
    assert!(!midway_colors.is_disjoint(&pale_colors));
    assert!(!midway_colors.is_disjoint(&red_colors));
    for contribution_total in [FULL_COLOR_CONTRIBUTIONS, ADULT_CONTRIBUTIONS] {
        assert!(collect_mix_colors(contribution_total, RED_MIX).is_disjoint(&pale_colors));
    }
}

#[test]
fn every_body_keeps_each_pigment_visible() {
    for koi_body in BODIES {
        let plain_frame = render_koi(
            &develop_with_languages(FULL_COLOR_CONTRIBUTIONS, &[]),
            koi_body,
        );
        for single_pigment_mix in [
            RED_MIX,
            &[("C", Some("#555555"), 1.0)],
            &[("JavaScript", Some("#f1e05a"), 1.0)],
            &[("Rust", Some("#dea584"), 1.0)],
            &[("Python", Some("#3572a5"), 1.0)],
        ] {
            let pigment_frame = render_koi(
                &develop_with_languages(FULL_COLOR_CONTRIBUTIONS, single_pigment_mix),
                koi_body,
            );
            let opaque_pixels: Vec<_> = plain_frame
                .as_rgba_bytes()
                .as_chunks::<4>()
                .0
                .iter()
                .zip(pigment_frame.as_rgba_bytes().as_chunks::<4>().0.iter())
                .filter(|(plain_pixel, _)| plain_pixel[3] != 0)
                .collect();
            let changed_pixel_count = opaque_pixels
                .iter()
                .filter(|(plain_pixel, pigment_pixel)| plain_pixel != pigment_pixel)
                .count();
            assert!(
                changed_pixel_count * 10 > opaque_pixels.len(),
                "{} nearly disappears on the {koi_body:?} body",
                single_pigment_mix[0].0
            );
        }
    }
}

#[test]
fn bodies_change_colors_but_not_shape() {
    let five_pigment_mix: LanguageMix = &[
        ("C++", Some("#f34b7d"), 0.2),
        ("C", Some("#555555"), 0.2),
        ("JavaScript", Some("#f1e05a"), 0.2),
        ("Rust", Some("#dea584"), 0.2),
        ("Python", Some("#3572a5"), 0.2),
    ];
    let koi_development = develop_with_languages(1_500, five_pigment_mix);
    let white_frame = render_koi(&koi_development, KoiBody::default());
    assert_eq!(white_frame, render_koi(&koi_development, KoiBody::White));
    let mut distinct_images = BTreeSet::new();
    for koi_body in BODIES {
        let koi_frame = render_koi(&koi_development, koi_body);
        for (white_pixel, body_pixel) in white_frame
            .as_rgba_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(koi_frame.as_rgba_bytes().as_chunks::<4>().0.iter())
        {
            assert_eq!(white_pixel[3], body_pixel[3]);
        }
        assert!(distinct_images.insert(koi_frame.as_rgba_bytes().to_vec()));
    }
}

#[test]
fn streak_fins_show_on_every_body() {
    let unmarked_development = develop_with_languages(FULL_COLOR_CONTRIBUTIONS, FOUR_LANGUAGE_MIX);
    let mut streaking_development =
        develop_with_languages(FULL_COLOR_CONTRIBUTIONS, FOUR_LANGUAGE_MIX);
    streaking_development.streaks.recent = Some(RecentStreak::Current(build_weeks(13)));
    for koi_body in BODIES {
        let unmarked_frame = render_koi(&unmarked_development, koi_body);
        let streaking_frame = render_koi(&streaking_development, koi_body);
        let pixel_pairs: Vec<_> = unmarked_frame
            .as_rgba_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(streaking_frame.as_rgba_bytes().as_chunks::<4>().0.iter())
            .collect();
        assert!(
            pixel_pairs
                .iter()
                .all(|(unmarked_pixel, streaking_pixel)| unmarked_pixel[3] == streaking_pixel[3])
        );
        let opaque_pixel_count = pixel_pairs
            .iter()
            .filter(|(unmarked_pixel, _)| unmarked_pixel[3] != 0)
            .count();
        let changed_pixel_count = pixel_pairs
            .iter()
            .filter(|(unmarked_pixel, streaking_pixel)| unmarked_pixel != streaking_pixel)
            .count();
        assert!(
            changed_pixel_count * 20 > opaque_pixel_count,
            "streak fins barely show on the {koi_body:?} body"
        );
    }
}

#[test]
fn the_longest_streak_changes_only_language_colors() {
    let render_with_longest = |language_mix, longest_week_count| {
        let mut koi_development = develop_with_languages(ADULT_CONTRIBUTIONS, language_mix);
        koi_development.streaks.longest = build_weeks(longest_week_count);
        render_koi(&koi_development, KoiBody::default())
    };
    let plain_frame = render_with_longest(&[], 4);
    assert_eq!(render_with_longest(&[], 0), plain_frame);
    assert_eq!(render_with_longest(&[], 208), plain_frame);
    let normal_frame = render_with_longest(FOUR_LANGUAGE_MIX, 4);
    assert_ne!(render_with_longest(FOUR_LANGUAGE_MIX, 0), normal_frame);
    assert_ne!(render_with_longest(FOUR_LANGUAGE_MIX, 208), normal_frame);
}

#[test]
fn growth_slows_and_keeps_the_whole_fish_inside_a_transparent_frame() {
    let mut koi_development = read_fixture_development();
    let mut visible_widths = BTreeMap::new();
    let mut visible_heights = BTreeMap::new();
    let mut area_per_squared_width = BTreeMap::new();
    let mut maximum_frame = None;
    for contribution_total in [
        0,
        500,
        1_000,
        1_500,
        2_000,
        12_750,
        25_000,
        50_000,
        75_000,
        100_000,
        u64::MAX,
    ] {
        koi_development.total_contributions = contribution_total;
        let koi_frame = render_koi(&koi_development, KoiBody::default());
        let opaque_bounds = measure_opaque_bounds(&koi_frame);
        assert!(opaque_bounds.left_x > 0 && opaque_bounds.top_y > 0);
        assert!(opaque_bounds.right_x < FRAME_WIDTH_PIXELS - 1);
        assert!(opaque_bounds.bottom_y < FRAME_HEIGHT_PIXELS - 1);
        let visible_width_pixels = opaque_bounds.right_x - opaque_bounds.left_x + 1;
        visible_widths.insert(contribution_total, visible_width_pixels);
        visible_heights.insert(
            contribution_total,
            opaque_bounds.bottom_y - opaque_bounds.top_y + 1,
        );
        let opaque_pixel_count = koi_frame
            .as_rgba_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel_channels| pixel_channels[3] != 0)
            .count();
        area_per_squared_width.insert(
            contribution_total,
            opaque_pixel_count as f64 / f64::from(visible_width_pixels).powi(2),
        );
        if contribution_total == 100_000 {
            maximum_frame = Some(koi_frame);
        } else if contribution_total > 100_000 {
            assert_eq!(Some(koi_frame), maximum_frame);
        }
    }
    let growth_widths: Vec<_> = visible_widths.values().copied().collect();
    assert!(
        growth_widths
            .windows(2)
            .all(|adjacent_widths| adjacent_widths[0] <= adjacent_widths[1])
    );
    assert!(
        visible_widths[&500] - visible_widths[&0] > visible_widths[&2_000] - visible_widths[&1_500]
    );
    for contribution_pair in [12_750, 25_000, 50_000, 75_000, 100_000].windows(2) {
        assert!(visible_widths[&contribution_pair[0]] < visible_widths[&contribution_pair[1]]);
        assert!(visible_heights[&contribution_pair[0]] < visible_heights[&contribution_pair[1]]);
    }
    assert!(area_per_squared_width[&100_000] > area_per_squared_width[&12_750]);
}

#[test]
fn swimming_keeps_the_whole_fish_inside_the_frame() {
    let mut koi_development = develop_with_languages(0, FOUR_LANGUAGE_MIX);
    for behavior in [
        Behavior::Cruise,
        Behavior::Dart,
        Behavior::Rest,
        Behavior::SurfaceFeeding,
        Behavior::DeepRest,
    ] {
        for contribution_total in [0, 1_500, ADULT_CONTRIBUTIONS, MAX_GROWTH_CONTRIBUTIONS] {
            koi_development.total_contributions = contribution_total;
            for frame_index in (0..frame_count(behavior)).step_by(10) {
                let opaque_bounds = measure_opaque_bounds(&render_swim_frame(
                    &koi_development,
                    KoiBody::default(),
                    behavior,
                    frame_index,
                ));
                assert!(
                    opaque_bounds.left_x > 0
                        && opaque_bounds.top_y > 0
                        && opaque_bounds.right_x < FRAME_WIDTH_PIXELS - 1
                        && opaque_bounds.bottom_y < FRAME_HEIGHT_PIXELS - 1,
                    "{behavior:?} at {contribution_total} contributions leaves the frame at \
                     frame {frame_index}"
                );
            }
        }
    }
}

#[test]
fn surface_feeding_draws_pellets_that_rest_does_not() {
    const PELLET_COLOR: [u8; 4] = [228, 166, 114, 255];
    let koi_development = develop_with_languages(1_500, FOUR_LANGUAGE_MIX);
    let colors_of = |behavior| {
        (0..frame_count(behavior))
            .step_by(5)
            .flat_map(|frame_index| {
                collect_opaque_colors(&render_swim_frame(
                    &koi_development,
                    KoiBody::default(),
                    behavior,
                    frame_index,
                ))
            })
            .collect::<BTreeSet<_>>()
    };
    assert!(colors_of(Behavior::SurfaceFeeding).contains(&PELLET_COLOR));
    assert!(!colors_of(Behavior::Rest).contains(&PELLET_COLOR));
}

#[test]
fn deeper_rest_turns_the_koi_toward_water_blue() {
    let koi_development = develop_with_languages(ADULT_CONTRIBUTIONS, FOUR_LANGUAGE_MIX);
    let measure_blue_lean = |behavior| {
        let swim_frame = render_swim_frame(&koi_development, KoiBody::default(), behavior, 0);
        let opaque_pixels: Vec<_> = swim_frame
            .as_rgba_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel_channels| pixel_channels[3] != 0)
            .copied()
            .collect();
        opaque_pixels
            .iter()
            .map(|pixel_channels| f64::from(pixel_channels[2]) - f64::from(pixel_channels[0]))
            .sum::<f64>()
            / opaque_pixels.len() as f64
    };
    let cruise_lean = measure_blue_lean(Behavior::Cruise);
    let rest_lean = measure_blue_lean(Behavior::Rest);
    let deep_rest_lean = measure_blue_lean(Behavior::DeepRest);
    assert!(
        rest_lean > cruise_lean + 10.0,
        "{cruise_lean} then {rest_lean}"
    );
    assert!(
        deep_rest_lean > rest_lean + 10.0,
        "{rest_lean} then {deep_rest_lean}"
    );
}

#[test]
fn the_swim_loop_is_seamless_and_never_jumps() {
    let koi_development = develop_with_languages(1_500, FOUR_LANGUAGE_MIX);
    let cruise_frame_count = frame_count(Behavior::Cruise);
    let swim_frames: Vec<_> = (0..=cruise_frame_count)
        .map(|frame_index| {
            render_swim_frame(
                &koi_development,
                KoiBody::default(),
                Behavior::Cruise,
                frame_index,
            )
        })
        .collect();
    assert_eq!(swim_frames[cruise_frame_count as usize], swim_frames[0]);
    let centroids: Vec<_> = swim_frames.iter().map(measure_opaque_centroid).collect();
    let centroid_steps: Vec<f64> = centroids
        .windows(2)
        .map(|centroid_pair| {
            (centroid_pair[1][0] - centroid_pair[0][0])
                .hypot(centroid_pair[1][1] - centroid_pair[0][1])
        })
        .collect();
    assert!(
        centroid_steps
            .iter()
            .all(|&centroid_step| centroid_step > 0.0 && centroid_step < 8.0),
        "{centroid_steps:?}"
    );
    let boundary_step = centroid_steps[cruise_frame_count as usize - 1];
    let nearby_steps =
        &centroid_steps[cruise_frame_count as usize - 6..cruise_frame_count as usize - 1];
    let nearby_average = nearby_steps.iter().sum::<f64>() / nearby_steps.len() as f64;
    assert!((0.5..1.5).contains(&(boundary_step / nearby_average)));
    let opaque_counts: Vec<_> = swim_frames
        .iter()
        .map(|swim_frame| collect_opaque_pixel_count(swim_frame) as f64)
        .collect();
    for count_pair in opaque_counts.windows(2) {
        assert!((count_pair[1] - count_pair[0]).abs() < 0.1 * count_pair[0]);
    }
}

#[test]
fn png_output_doubles_each_pixel_and_repeats_exactly() {
    let mut koi_development = read_fixture_development();
    koi_development.total_contributions = 1_500;
    let koi_frame = render_koi(&koi_development, KoiBody::default());
    let png_bytes = encode_png(&koi_frame).unwrap();
    assert_eq!(
        png_bytes,
        encode_png(&render_koi(&koi_development, KoiBody::default())).unwrap()
    );
    let mut png_reader = png::Decoder::new(Cursor::new(&png_bytes))
        .read_info()
        .unwrap();
    let mut decoded_pixels = vec![0; png_reader.output_buffer_size().unwrap()];
    let frame_info = png_reader.next_frame(&mut decoded_pixels).unwrap();
    assert_eq!((frame_info.width, frame_info.height), (1536, 512));
    assert_eq!(frame_info.color_type, png::ColorType::Rgba);
    for (pixel_index, native_channels) in koi_frame
        .as_rgba_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
    {
        let block_left = pixel_index as u32 % FRAME_WIDTH_PIXELS * 2;
        let block_top = pixel_index as u32 / FRAME_WIDTH_PIXELS * 2;
        for (offset_x, offset_y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let export_index =
                ((block_top + offset_y) * frame_info.width + block_left + offset_x) as usize * 4;
            assert_eq!(
                &decoded_pixels[export_index..export_index + 4],
                native_channels
            );
        }
    }
}

#[test]
fn quiet_days_change_streak_state_without_changing_the_anatomy() {
    let mut account_snapshot: AccountSnapshot =
        serde_json::from_str(include_str!("fixtures/account-with-history.json")).unwrap();
    let active_development = derive_koi_development(&account_snapshot).unwrap();
    let contribution_history = account_snapshot.contribution_history.as_mut().unwrap();
    for day_offset in 1..=14 {
        contribution_history.days.push(ContributionDay {
            date: contribution_history
                .observed_on
                .checked_add_days(Days::new(day_offset))
                .unwrap(),
            contribution_count: 0,
        });
    }
    contribution_history.observed_on = contribution_history.days.last().unwrap().date;
    let quiet_development = derive_koi_development(&account_snapshot).unwrap();
    assert_ne!(active_development.streaks, quiet_development.streaks);
    let active_frame = render_koi(&active_development, KoiBody::default());
    let quiet_frame = render_koi(&quiet_development, KoiBody::default());
    for (active_pixel, quiet_pixel) in active_frame
        .as_rgba_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(quiet_frame.as_rgba_bytes().as_chunks::<4>().0.iter())
    {
        assert_eq!(active_pixel[3], quiet_pixel[3]);
    }
}

fn read_fixture_development() -> KoiDevelopment {
    let account_snapshot =
        serde_json::from_str(include_str!("fixtures/account-with-history.json")).unwrap();
    derive_koi_development(&account_snapshot).unwrap()
}

fn develop_with_languages(contribution_total: u64, language_mix: LanguageMix) -> KoiDevelopment {
    let mut koi_development = read_fixture_development();
    koi_development.total_contributions = contribution_total;
    koi_development.language_shares = language_mix
        .iter()
        .map(|&(language_name, _, language_share)| (language_name.to_owned(), language_share))
        .collect();
    koi_development.language_colors = language_mix
        .iter()
        .map(|&(language_name, color_text, _)| {
            (
                language_name.to_owned(),
                color_text.map(|color_text| LanguageColor::try_from(color_text).unwrap()),
            )
        })
        .collect();
    // Normal colors and no streak fins: a four-week record that ended long ago.
    koi_development.streaks.longest = build_weeks(4);
    koi_development.streaks.recent = Some(RecentStreak::Ended {
        weeks: build_weeks(4),
        ended_on: koi_development.observed_on - Days::new(70),
    });
    koi_development
}

/// Fully active weeks; marks depend only on how many there are.
fn build_weeks(week_count: usize) -> Vec<ContributionWeek> {
    let first_monday: NaiveDate = "2024-01-01".parse().unwrap();
    (0..week_count)
        .map(|week_index| ContributionWeek {
            iso_week: (first_monday + Days::new(7 * week_index as u64)).iso_week(),
            active_day_count: 7,
            observed_day_count: 7,
        })
        .collect()
}

fn collect_opaque_pixel_count(koi_frame: &KoiFrame) -> usize {
    koi_frame
        .as_rgba_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel_channels| pixel_channels[3] != 0)
        .count()
}

fn measure_opaque_centroid(koi_frame: &KoiFrame) -> [f64; 2] {
    let mut position_sum = [0.0; 2];
    let mut opaque_pixel_count = 0.0;
    for (pixel_index, pixel_channels) in koi_frame
        .as_rgba_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
    {
        if pixel_channels[3] != 0 {
            position_sum[0] += (pixel_index as u32 % FRAME_WIDTH_PIXELS) as f64;
            position_sum[1] += (pixel_index as u32 / FRAME_WIDTH_PIXELS) as f64;
            opaque_pixel_count += 1.0;
        }
    }
    [
        position_sum[0] / opaque_pixel_count,
        position_sum[1] / opaque_pixel_count,
    ]
}

fn collect_opaque_colors(koi_frame: &KoiFrame) -> BTreeSet<[u8; 4]> {
    koi_frame
        .as_rgba_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel_channels| pixel_channels[3] != 0)
        .copied()
        .collect()
}

struct OpaqueBounds {
    left_x: u32,
    top_y: u32,
    right_x: u32,
    bottom_y: u32,
}

fn measure_opaque_bounds(koi_frame: &KoiFrame) -> OpaqueBounds {
    let mut opaque_bounds = OpaqueBounds {
        left_x: FRAME_WIDTH_PIXELS,
        top_y: FRAME_HEIGHT_PIXELS,
        right_x: 0,
        bottom_y: 0,
    };
    for (pixel_index, pixel_channels) in koi_frame
        .as_rgba_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .enumerate()
    {
        if pixel_channels[3] == 0 {
            continue;
        }
        let pixel_x = pixel_index as u32 % FRAME_WIDTH_PIXELS;
        let pixel_y = pixel_index as u32 / FRAME_WIDTH_PIXELS;
        opaque_bounds.left_x = opaque_bounds.left_x.min(pixel_x);
        opaque_bounds.top_y = opaque_bounds.top_y.min(pixel_y);
        opaque_bounds.right_x = opaque_bounds.right_x.max(pixel_x);
        opaque_bounds.bottom_y = opaque_bounds.bottom_y.max(pixel_y);
    }
    assert!(
        opaque_bounds.left_x < opaque_bounds.right_x
            && opaque_bounds.top_y < opaque_bounds.bottom_y
    );
    opaque_bounds
}
