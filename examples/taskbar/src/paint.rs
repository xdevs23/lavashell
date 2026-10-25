// SPDX-License-Identifier: GPL-3.0-only
use lavashell::prelude::*;

const STREET: Color = Color::oklch(0.3, 0.024, 258.0);
const PARK: Color = Color::oklch(0.27, 0.05, 155.0);
const WATER: Color = Color::oklch(0.33, 0.08, 234.0);
const ROUTE: Color = Color::oklch(0.84, 0.17, 158.0);

const ACROSS: Point = Point::new(Dp::new(96.0), Dp::new(-10.0));
const ALONG: Point = Point::new(Dp::new(16.0), Dp::new(90.0));
const ORIGIN: Point = Point::new(Dp::new(-60.0), Dp::ZERO);
const BLOCK: f64 = 0.9;
const BLOCK_BESIDE_WIDE_STREET: f64 = 0.8;
const WIDE_STREET_COLUMNS: i16 = 5;
const WIDE_STREET_ROWS: i16 = 4;
const PARK_SPREAD: [i16; 2] = [3, 5];
const SHADE_SPREAD: [i16; 2] = [7, 13];
const PARKS: i16 = 11;
const SHADES: i16 = 5;
const SHADE_STEP: f64 = 0.006;

fn corner(i: f64, j: f64) -> Point {
    Point::new(
        ORIGIN.x + ACROSS.x * i + ALONG.x * j,
        ORIGIN.y + ACROSS.y * i + ALONG.y * j,
    )
}

pub fn map(d: &mut Drawing<'_>) {
    let size = d.size();
    d.fill(Rect::new(0.dp(), 0.dp(), size.width, size.height), STREET);
    for i in blocks(-2, ACROSS.x, size.width) {
        for j in blocks(-1, ALONG.y, size.height) {
            block(d, i, j);
        }
    }
    let river = river(size);
    d.path(&river, false, WATER.with_alpha(0.5), Some(80.dp()));
    d.path(&river, false, WATER, Some(60.dp()));
    let route = [
        corner(4.0, 6.0),
        corner(4.0, 3.0),
        corner(8.0, 3.0),
        corner(8.0, 2.0),
        corner(11.0, 2.0),
    ];
    d.path(&route, false, ROUTE.with_alpha(0.2), Some(22.dp()));
    d.path(&route, false, ROUTE, Some(7.dp()));
    let car = corner(4.0, 6.0);
    d.circle(
        car,
        28.dp(),
        Color::oklch(0.7, 0.12, 250.0).with_alpha(0.18),
    );
    d.circle(car, 11.dp(), Color::WHITE);
    d.circle(car, 6.5.dp(), Color::oklch(0.6, 0.19, 258.0));
    let destination = corner(11.0, 2.0);
    d.circle(destination, 15.dp(), ROUTE);
    d.circle(destination, 6.dp(), Color::WHITE);
}

fn blocks(first: i16, step: Dp, length: Dp) -> impl Iterator<Item = i16> {
    (first..).take_while(move |&index| step * f64::from(index) <= length + step * 2.0)
}

fn block(d: &mut Drawing<'_>, column: i16, row: i16) {
    let (left, top) = (f64::from(column), f64::from(row));
    let center = corner(left + 0.5, top + 0.5);
    let wide = column.rem_euclid(WIDE_STREET_COLUMNS) == 0 || row.rem_euclid(WIDE_STREET_ROWS) == 0;
    let size = if wide {
        BLOCK_BESIDE_WIDE_STREET
    } else {
        BLOCK
    };
    let at = |a: f64, b: f64| {
        let at = corner(a, b);
        Point::new(
            center.x + (at.x - center.x) * size,
            center.y + (at.y - center.y) * size,
        )
    };
    let outline = [
        at(left, top),
        at(left + 1.0, top),
        at(left + 1.0, top + 1.0),
        at(left, top + 1.0),
    ];
    let spread =
        |[across, along]: [i16; 2], period: i16| (column * across + row * along).rem_euclid(period);
    let color = if spread(PARK_SPREAD, PARKS) == 0 {
        PARK
    } else {
        Color::oklch(
            0.2 + f64::from(spread(SHADE_SPREAD, SHADES)) * SHADE_STEP,
            0.03,
            262.0,
        )
    };
    d.path(&outline, true, color, None);
}

fn river(size: Size) -> Vec<Point> {
    const SAMPLES: u8 = 48;
    const FROM: f64 = -0.1;
    const SPAN: f64 = 1.2;
    const LEVEL: f64 = 0.86;
    const AMPLITUDE: f64 = 0.04;
    const FREQUENCY: f64 = 6.0;
    const SLOPE: f64 = 0.06;
    (0..=SAMPLES)
        .map(|sample| {
            let x = f64::from(sample) / f64::from(SAMPLES) * SPAN + FROM;
            let y = LEVEL + AMPLITUDE * (x * FREQUENCY).sin() - SLOPE * x;
            Point::new(size.width * x, size.height * y)
        })
        .collect()
}
