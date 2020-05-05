use rand::{thread_rng, Rng, random};
use rand::seq::SliceRandom;
use rand::seq::IteratorRandom;
use rand::distributions::{Distribution, Standard};


use std::fmt::{self, Display, Write};
use std::collections::{HashSet, HashMap};
use std::time::{Instant, Duration};

use crate::json::Json;

const BLOCK_SIZE_PIXELS: usize = 10;

const WIDTH: usize = 8;
const HEIGHT: usize = 7;
const N_COLORS: u8 = 6;
const DEPTH: usize = 4;

pub fn new_game_state() -> Json {
    GameState::new().jsonify()
}

pub fn handle_request(json: &Json) -> Option<Json> {
    let request = json.get_object()?;
    let mut game_state = GameState::from_json(request.get("state")?)?;
    let color_chosen = Color::from_json(request.get("move")?)?;

    game_state.do_move(color_chosen).ok()?;

    game_state.do_move(
        game_state.get_colors().into_iter()
            .map(|color| {
                let mut next = game_state.clone();
                next.do_move(color).unwrap();
                let evaluation = max_advantage(next, false, false, DEPTH);
                (color, evaluation)
            })
            .max_by_key(|&(_, e)| e)
            .unwrap()
            .0
    ).ok()?;

    Some(game_state.jsonify())
}


fn max_advantage(mut game_state: GameState, is_left: bool, is_our_turn: bool, depth_left: usize) -> isize {
    if depth_left == 0 {
        game_state.left_advantage() * if is_left { 1 } else { -1 }
    } else {
        let a = game_state.get_colors().into_iter()
            .map(|c| {
                let mut new_game_state = game_state.clone();
                new_game_state.do_move(c).unwrap();
                max_advantage(new_game_state, is_left, !is_our_turn, depth_left-1)
            });

        if is_our_turn {
            a.max().unwrap()
        } else {
            a.min().unwrap()
        }
    }
}


#[derive(Clone)]
struct GameState {
    field: Field,
    left_territory: HashSet<(usize, usize)>,
    right_territory: HashSet<(usize, usize)>,
    is_left_turn: bool,
}

impl GameState {
    pub fn new() -> GameState {
        GameState {
            field: Field::from_random(),
            left_territory: vec![(0, HEIGHT-1)].into_iter().collect(),
            right_territory: vec![(WIDTH-1, 0)].into_iter().collect(),
            is_left_turn: true,
        }
    }

    pub fn get_colors(&self) -> Vec<Color> {
        let mut reasonable = self.reasonable_colors();

        if reasonable.is_empty() {
            self.valid_colors().to_vec()
        } else {
            reasonable.into_iter().collect()
        }
    }

    pub fn jsonify(&self) -> Json {
        let mut map = HashMap::new();

        map.insert("field".to_string(), self.field.jsonify());

        map.insert(
            "leftTerritory".to_string(),
            Json::Array(self.left_territory.iter()
                .map(|&(x, y)|
                    Json::Object(
                        [
                            ("x".to_string(), Json::Number(x as f64)),
                            ("y".to_string(), Json::Number(y as f64))
                        ].iter().cloned().collect()
                    )
                )
                .collect()
            )
        );
        map.insert(
            "rightTerritory".to_string(),
            Json::Array(self.right_territory.iter()
                .map(|&(x, y)|
                    Json::Object(
                        [
                            ("x".to_string(), Json::Number(x as f64)),
                            ("y".to_string(), Json::Number(y as f64))
                        ].iter().cloned().collect()
                    )
                )
                .collect()
            )
        );

        map.insert("isLeftTurn".to_string(), Json::Boolean(self.is_left_turn));

        Json::Object(map)
    }

    pub fn from_json(json: &Json) -> Option<GameState> {
        let map = json.get_object()?;

        let field = Field::from_json(map.get("field")?)?;
        let left_territory = map.get("leftTerritory")?.get_array()?.iter()
            .map(|point_json| {
                let point = point_json.get_object()?;
                Some((point.get("x")?.get_number()? as usize, point.get("y")?.get_number()? as usize))
            })
            .collect::<Option<_>>()?;

        let right_territory = map.get("rightTerritory")?.get_array()?.iter()
            .map(|point_json| {
                let point = point_json.get_object()?;
                Some((point.get("x")?.get_number()? as usize, point.get("y")?.get_number()? as usize))
            })
            .collect::<Option<_>>()?;

        let is_left_turn = map.get("isLeftTurn")?.get_bool()?;

        Some(GameState { field, left_territory, right_territory, is_left_turn })
    }

    pub fn reasonable_colors(&self) -> HashSet<Color> {
        let territory =
            if self.is_left_turn {
                &self.left_territory
            } else {
                &self.right_territory
            };

        let mut surrounding_colors = HashSet::new();

        for &(x, y) in territory.iter() {
            // we use wrapping sub mostly because i'm lazy and it works because if x == 0 and we do a wrapping sub,
            // we're gonna to get a None value from our field.get
            for &(around_x, around_y) in [(x, y.wrapping_sub(1)), (x, y + 1), (x.wrapping_sub(1), y), (x + 1, y)].iter() {
                if let Some(color) = self.field.get(around_x, around_y) {
                    if color != self.left_color() && color != self.right_color() {
                        surrounding_colors.insert(color);
                    }
                }
            }
        }
        surrounding_colors
    }

    fn left_color(&self) -> Color {
        let &(x, y) = self.left_territory.iter().next().unwrap();
        self.field.get(x, y).unwrap()
    }

    fn right_color(&self) -> Color {
        let &(x, y) = self.right_territory.iter().next().unwrap();
        self.field.get(x, y).unwrap()
    }

    pub fn valid_colors(&self) -> [Color; 4] {
        let mut ret = [Color::Black; 4];
        let mut index = 0;

        for i in 1..=6 {
            let color = Color::from_u8(i);
            if color != self.left_color() && color != self.right_color() {
                ret[index] = color;
                index += 1;
            }
        }

        ret
    }

    pub fn is_over(&self) -> bool {
        self.left_territory.len() + self.right_territory.len() == WIDTH * HEIGHT
    }

    pub fn left_advantage(&self) -> isize {
        self.left_territory.len() as isize - self.right_territory.len() as isize
    }

    pub fn left_winning(&self) -> bool {
        // only call if is over
        self.left_advantage() > 0
    }

    pub fn do_move(&mut self, fill_color: Color) -> Result<(), ()> {
        // check if fill_color is valid (ie. not our or opponents current color)
        if fill_color == self.left_color() || fill_color == self.right_color() {
            return Err(());
        }

        let territory =
            if self.is_left_turn {
                &mut self.left_territory
            } else {
                &mut self.right_territory
            };

        let mut to_add = HashSet::new();

        for &(x, y) in territory.iter() {
            // we use wrapping sub mostly because i'm lazy and it works because if x == 0 and we do a wrapping sub,
            // we're gonna to get a None value from our field.get
            for &(around_x, around_y) in [(x, y.wrapping_sub(1)), (x, y+1), (x.wrapping_sub(1), y), (x+1, y)].iter() {
                if self.field.get(around_x, around_y) == Some(fill_color) {
                    // yes, this value might already be in to_add, but that's fine cause this is a set
                    to_add.insert((around_x, around_y));
                }
            }
        }

        territory.extend(to_add.into_iter());

        for &(x, y) in territory.iter() {
            self.field.set(x, y, fill_color);
        }

        self.is_left_turn = !self.is_left_turn;

        Ok(())
    }


}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
enum Color {
    Red = 1,
    Yellow = 2,
    Green = 3,
    Blue = 4,
    Purple = 5,
    Black = 6,
}

impl Color {
    fn from_u8(n: u8) -> Color {
        match n {
            1 => Color::Red,
            2 => Color::Yellow,
            3 => Color::Green,
            4 => Color::Blue,
            5 => Color::Purple,
            6 => Color::Black,
            _ => panic!("color index out of range"),
        }
    }

    fn jsonify(self) -> Json {
        match self {
            Color::Red => Json::String(String::from("red")),
            Color::Yellow => Json::String(String::from("yellow")),
            Color::Green => Json::String(String::from("green")),
            Color::Blue => Json::String(String::from("blue")),
            Color::Purple => Json::String(String::from("purple")),
            Color::Black => Json::String(String::from("black")),
        }
    }

    fn from_json(json: &Json) -> Option<Color> {
        Some(match json.get_string()? {
            "red" => Color::Red,
            "yellow" => Color::Yellow,
            "green" => Color::Green,
            "blue" => Color::Blue,
            "purple" => Color::Purple,
            "black" => Color::Black,
            _ => return None,
        })
    }
}

impl Distribution<Color> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Color {
        let n = rng.gen_range(1, N_COLORS+1);

        Color::from_u8(n)
    }
}


#[derive(Clone)]
struct Field {
    inner: [[Color; WIDTH]; HEIGHT],
}

impl Field {
    pub fn jsonify(&self) -> Json {
        Json::Array(
            self.inner.iter()
                .map(|row|
                    Json::Array(row.iter().map(|color| color.jsonify()).collect::<Vec<Json>>()))
                .collect::<Vec<Json>>()
        )
    }

    pub fn from_json(json: &Json) -> Option<Field> {
        let mut inner = [[Color::Black; WIDTH]; HEIGHT];

        for (y, rows) in json.get_array()?.iter().enumerate() {
            for (x, color) in rows.get_array()?.iter().enumerate() {
                *inner.get_mut(y)?.get_mut(x)? = Color::from_json(color)?;
            }
        }

        Some(Field { inner })
    }

    pub fn from_random() -> Field {
        let mut inner = [[Color::Black; WIDTH]; HEIGHT]; // not gonna stay black

        for y in 0..HEIGHT {
            for x in 0..WIDTH {

                // we want a color such that the colors above and to the left are different
                // if this is true for every color on the map, then we get a checkerboard deelio
                // (no two adjacent colors are the same)
                let mut color: Color;

                loop {
                    color = random();

                    // extra check: bottom right and upper left cannot be the same color
                    if x == 0 && y == HEIGHT-1 && color == inner[0][WIDTH-1] { continue }

                    if (x!=0 && y!=0 && inner[y][x-1] != color && inner[y-1][x] != color)
                        || (x==0 && y==0)
                        || (x==0 && inner[y-1][x] != color)
                        || (y==0 && inner[y][x-1] != color)
                    { break }
                }

                inner[y][x] = color;
            }
        }

        Field { inner }
    }

    pub fn get(&self, x: usize, y: usize) -> Option<Color> {
        self.inner.get(y)?.get(x).copied()
    }

    pub fn set(&mut self, x: usize, y: usize, color: Color) {
        self.inner[y][x] = color;
    }

    pub fn rotate(&self) -> Field {
        let mut new_inner = [[Color::Black; WIDTH]; HEIGHT];

        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                new_inner[y][x] = self.inner[HEIGHT-y-1][WIDTH-x-1];
            }
        }

        Field { inner: new_inner }
    }
}

// fn interactive() {
//     let mut window: PistonWindow<GlutinWindow> = WindowSettings::new(
//         "Filler",
//         [(WIDTH*BLOCK_SIZE_PIXELS) as u32, (HEIGHT*BLOCK_SIZE_PIXELS) as u32],
//     )
//         .exit_on_esc(true)
//         .build()
//         .unwrap();
//
//     let mut game_state = GameState::new();
//
//     let mut blink_on = true;
//     let mut last_blink = Instant::now();
//
//     let mut cursor_x = 0;
//     let mut cursor_y = 0;
//
//     while let Some(e) = window.next() {
//         window.draw_2d(&e, |c, g, _| {
//             if last_blink.elapsed() > Duration::from_millis(750) {
//                 last_blink = Instant::now();
//                 blink_on = !blink_on;
//             }
//
//             draw_game_state(&game_state, blink_on, g, &c);
//         });
//
//         e.mouse_cursor(|[x, y]| {
//             cursor_x = x as usize;
//             cursor_y = y as usize;
//         });
//
//
//         e.press(|args| {
//             if let Button::Mouse(mouse_button) = args {
//                 if game_state.is_left_turn && mouse_button == MouseButton::Left {
//                     let color_selected = game_state.field.get(cursor_x/BLOCK_SIZE_PIXELS, cursor_y/BLOCK_SIZE_PIXELS).unwrap();
//
//                     if game_state.do_move(color_selected).is_err() {
//                         println!("invalid move");
//                         return;
//                     }
//
//                     // enemy turn
//
//                     println!("left {}\nright {}\n", game_state.left_territory.len(), game_state.right_territory.len());
//                 }
//             }
//         });
//     }
// }
