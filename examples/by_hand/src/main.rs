//! Calls the function `build.rs` translated from CEL, as ordinary Rust.

pub struct Sensor {
    pub temp: f64,
    pub limit: f64,
}

mod generated {
    include!(concat!(env!("OUT_DIR"), "/status.rs"));
}

fn main() -> Result<(), cel_emit_runtime::Error> {
    let cold = Sensor {
        temp: 18.5,
        limit: 30.0,
    };
    let warm = Sensor {
        temp: 41.0,
        limit: 30.0,
    };
    println!("{}", generated::status(&cold)?);
    println!("{}", generated::status(&warm)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_expression_runs_as_rust() {
        let warm = Sensor {
            temp: 41.0,
            limit: 30.0,
        };
        assert_eq!(generated::status(&warm), Ok("hot".to_string()));
    }
}
