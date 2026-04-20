use std::{env::args, error::Error};

use easy_sat_rs::{SolveResult, SolverBuilder, search::Searcher};

fn main() -> Result<(), Box<dyn Error>> {
    let args = args().collect::<Vec<String>>();
    let cnf_path = args.get(1).unwrap();

    let search = Searcher;
    let solver = SolverBuilder::from_dimacs_file(search, cnf_path)?.build();

    match solver.solve() {
        SolveResult::SAT(solver) => {
            solver.check_sat()?;
            println!("s SATISFIABLE");
            solver.print_model();
        }
        SolveResult::UNSAT(_) => {
            println!("s UNSATISFIABLE");
        }
        SolveResult::UNKNOWN(_) => {
            println!("s UNKNOWN");
        }
    }

    Ok(())
}
