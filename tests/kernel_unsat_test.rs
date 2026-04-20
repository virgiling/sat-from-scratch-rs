use easy_sat_rs::{Solver, SolverStatus, kernel::Kernel, search::Searcher};

fn add_clause(kernel: &mut Kernel, lits: &[isize]) {
    for &lit in lits {
        kernel.add(Some(lit));
    }
    kernel.add(None);
}

fn pigeon_var(pigeon: usize, hole: usize, holes: usize) -> isize {
    (pigeon * holes + hole + 1) as isize
}

fn add_pigeonhole_3_2(kernel: &mut Kernel) {
    let pigeons = 3usize;
    let holes = 2usize;

    // Every pigeon sits in at least one hole.
    for p in 0..pigeons {
        let mut clause = Vec::with_capacity(holes);
        for h in 0..holes {
            clause.push(pigeon_var(p, h, holes));
        }
        add_clause(kernel, &clause);
    }

    // No hole contains two different pigeons.
    for h in 0..holes {
        for p1 in 0..pigeons {
            for p2 in (p1 + 1)..pigeons {
                add_clause(kernel, &[-pigeon_var(p1, h, holes), -pigeon_var(p2, h, holes)]);
            }
        }
    }
}

#[test]
fn test_solver_unsat_on_pigeonhole() {
    let mut kernel = Kernel::new(6);
    add_pigeonhole_3_2(&mut kernel);

    let solver = Solver::new(Searcher, kernel);
    let result = solver.solve();
    assert_eq!(result.status(), SolverStatus::UNSAT);
}
