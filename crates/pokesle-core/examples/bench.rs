//! 速度確認: cargo run -p pokesle-core --release --example bench
use std::time::Instant;

use pokesle_core::config::Config;
use pokesle_core::evaluate_table::simulate_evaluate_table;
use pokesle_core::sim_pokemon::BoxPokemon;
use pokesle_core::team_sim::{run_team_simulation, TeamSimulationRequest};

fn main() {
    let golden: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string("crates/pokesle-core/tests/golden/box.json").unwrap()).unwrap();
    let mut box_list: Vec<BoxPokemon> = serde_json::from_value(golden["box"].clone()).unwrap();
    for (i, b) in box_list.iter_mut().enumerate() {
        b.index = i;
    }
    let mut config = Config::default_config();
    config.worker_num = std::thread::available_parallelism().map(|n| n.get() as f64).unwrap_or(4.0);
    println!("threads={} box={}", config.worker_num, box_list.len());

    let t = Instant::now();
    let request = TeamSimulationRequest { target_day: None, target_hour: 24.0, initial_cooking_change: 0.0, initial_cooking_power_up: 0.0, before_energy: 0.0 };
    let r = run_team_simulation(&config, &box_list, None, &request, &|_| {}).unwrap();
    println!("team: {:.2}s best={:.0}", t.elapsed().as_secs_f64(), r.team_list[0].score);

    if std::env::args().any(|a| a == "--table") {
        let t = Instant::now();
        let table = simulate_evaluate_table(&config, &()).unwrap();
        println!("evaluate table: {:.2}s pokemon={}", t.elapsed().as_secs_f64(), table.pokemon.len());
    }
}
