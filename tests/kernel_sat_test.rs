use easy_sat_rs::{Solver, UNKNOWN, api::Search, kernel::Kernel, search::Searcher};
use rstest::{fixture, rstest};

fn add_clause(kernel: &mut Kernel, lits: &[isize]) {
    for &lit in lits {
        kernel.add(Some(lit));
    }
    kernel.add(None);
}

fn print_model(model: &[bool]) {
    print!("v ");
    for v in 1..model.len() {
        if model[v] {
            print!("{} ", v);
        } else {
            print!("-{} ", v);
        }
        if v % 10 == 0 {
            print!("\nv");
        }
    }
    println!("0");
}

#[fixture]
fn construct_cnf() -> Kernel {
    let mut kernel = Kernel::new(7);
    add_clause(&mut kernel, &[-5, 7]);
    add_clause(&mut kernel, &[-1, -5, 6]);
    add_clause(&mut kernel, &[-1, -6, -7]);
    add_clause(&mut kernel, &[-1, -2, 5]);
    add_clause(&mut kernel, &[-1, -3, 5]);
    add_clause(&mut kernel, &[-1, -4, 5]);
    add_clause(&mut kernel, &[-2, -3, -4, 5]);
    add_clause(&mut kernel, &[-1, 2, 3, 4, 5, -6]);
    kernel
}

#[rstest]
fn test_search_functions_on_given_cnf(construct_cnf: Kernel) {
    let mut kernel = construct_cnf;
    let mut searcher = Searcher;

    // Exercise `propagate` once on this CNF.
    kernel.level = 1;
    kernel.assign(1, 1, None);
    let _ = searcher.propagate(&mut kernel);

    // Build a deterministic conflict state to exercise `analyze` + `backtrack`:
    // conflict clause is [-1, -6, -7] (index 2), with x1=true@L1, x6=true@L0, x7=true@L0.
    kernel.assignment.fill(0);
    kernel.vars.fill(Default::default());
    kernel.trail.clear();
    kernel.assigned = 0;
    kernel.propagated = 0;
    kernel.level = 0;
    kernel.assign(6, 6, None);
    kernel.assign(7, 7, None);
    kernel.level = 1;
    kernel.assign(1, 1, None);
    kernel.conflict = Some((2, -1));

    let trail_before = kernel.trail.len();
    let clauses_before = kernel.clauses.len();
    searcher.analyze(&mut kernel);
    assert_eq!(kernel.clauses.len(), clauses_before + 1, "analyze should add one learned clause");
    assert!(
        kernel.backtrack_level <= kernel.level,
        "backtrack level should not exceed current level"
    );

    searcher.backtrack(&mut kernel);
    assert!(kernel.trail.len() <= trail_before, "backtrack should not increase trail length");
}

#[rstest]
fn test_solver_sat_on_given_cnf(construct_cnf: Kernel) {
    let kernel = construct_cnf;
    let searcher = Searcher;
    let solver = Solver::<Searcher, UNKNOWN>::new(searcher, kernel);
    let result = solver.solve();
    match result {
        easy_sat_rs::SolveResult::SAT(sat_solver) => {
            assert!(sat_solver.check_sat().is_ok());
            print_model(&sat_solver.model());
        }
        _ => panic!("expected SAT"),
    }
}
