// flea --clip: the terminal and test seam over the clipboard; shape errors belong to usage() in main.rs.
use super::control;
use super::own;
use super::reply;
use super::x11;

pub fn get() -> i32 {
    let result = if super::use_x11() { x11::get() } else { control::get() };
    match result {
        Ok(got) => {
            println!("{}", reply::reply_get(Ok(&got)));
            0
        }
        Err(e) => {
            println!("{}", reply::reply_get(Err(e.as_str())));
            2
        }
    }
}

pub fn set(op: &str) -> i32 {
    let input = match own::read_capped_stdin() {
        Ok(input) => input,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    let paths = match own::nul_paths(&input) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("flea: {}", e);
            return 2;
        }
    };
    let result = if super::use_x11() { x11::set(op, &paths) } else { own::spawn_owner(op, &paths) };
    match result {
        Ok(token) => {
            println!("{}", token);
            0
        }
        Err(e) => {
            eprintln!("flea: {}", e);
            2
        }
    }
}

pub fn clear(token: &str) -> i32 {
    if token.is_empty() {
        eprintln!("flea: the token names the copy to clear");
        return 2;
    }
    let result = if super::use_x11() { x11::clear(token) } else { control::clear(token, None) };
    match result {
        Ok(cleared) => {
            println!("{}", reply::reply_clear(Ok(cleared)));
            0
        }
        Err(e) => {
            println!("{}", reply::reply_clear(Err(e.as_str())));
            2
        }
    }
}
