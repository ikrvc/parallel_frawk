use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use crate::ast;
use crate::ast::{Expr, Stmt};
use crate::parallelization::ast_check_parallelization::{valid_lhs};
use crate::parallelization::find_global::{GlobalVar, IndexVal};

pub fn find_truly_globals<'a, 'b, I: Clone + Hash + Eq+Debug>(program: &ast::Prog<'a, 'b, I>, global_vars: &HashSet<GlobalVar<'a, I>>) -> HashSet<GlobalVar<'a, I>> {
    let mut truly_globals = HashSet::new();
    let mut usages = HashMap::new();
    for (_, maybe_stmt) in &program.pats {
        match maybe_stmt {
            Some(stmt) => {
                let mut stack = Vec::new();
                stack.push(HashMap::new());
                check_statement_for_locality(stmt, global_vars, &mut usages, &mut truly_globals, &mut stack);
            },
            None => continue,
        }
    }
    let mut queue = usages.keys().cloned().collect::<Vec<_>>();
    while !queue.is_empty() {
        let k = queue.pop().unwrap();
        if truly_globals.contains(&k) {
            if let Some(usage) = usages.get(&k) {
                for var in usage {
                    if !truly_globals.contains(var) {
                        truly_globals.insert(var.clone());
                        queue.push(var.clone());
                    }
                }
            }
        }

    }
    println!("Truly globals: {:?}", truly_globals);
    truly_globals
}

fn check_statement_for_locality<'a, I: Clone + Hash + Eq+Debug>(stmt: &Stmt<'_, 'a, I>, global_vars: &HashSet<GlobalVar<'a, I>>, usages: &mut HashMap<GlobalVar<'a, I>, HashSet<GlobalVar<'a, I>>>, truly_globals: &mut HashSet<GlobalVar<'a, I>>, dependency_stack: &mut Vec<HashMap<GlobalVar<'a, I>, HashSet<GlobalVar<'a, I>>>>) {
    match stmt {
        Stmt::Expr(expr) => {check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack);},
        Stmt::Block(vec) => {
            for stmt in vec {
                check_statement_for_locality(stmt, global_vars, usages, truly_globals, dependency_stack);
            }
        }
        Stmt::If(cond, stmt1, stmt2) => {
            check_expression_for_locality(cond, global_vars, usages, truly_globals, dependency_stack);
            dependency_stack.push(HashMap::new()); //add new branch scope
            check_statement_for_locality(stmt1, global_vars, usages, truly_globals, dependency_stack);
            let assign_in_if = dependency_stack.pop().unwrap();
            if let Some(stmt) = stmt2 {
                dependency_stack.push(HashMap::new());
                check_statement_for_locality(stmt, global_vars, usages, truly_globals, dependency_stack);
                let assign_in_else = dependency_stack.pop().unwrap();
                connect_environments(assign_in_if, assign_in_else, dependency_stack)
            }
        },
        Stmt::While(_, cond, stmt) => {
            check_expression_for_locality(cond, global_vars, usages, truly_globals, dependency_stack);
            dependency_stack.push(HashMap::new()); //add new branch scope
            check_statement_for_locality(stmt, global_vars, usages, truly_globals, dependency_stack);
            dependency_stack.pop();
        },
        Stmt::DoWhile(cond, stmt) => {
            dependency_stack.push(HashMap::new()); //add new branch scope
            check_statement_for_locality(stmt, global_vars, usages, truly_globals, dependency_stack);
            dependency_stack.pop();
            check_expression_for_locality(cond, global_vars, usages, truly_globals, dependency_stack);
        },
        Stmt::For(stmt1, cond, stmt2, stmt3) => {
            if let Some(stmt) = stmt1 {
                check_statement_for_locality(stmt, global_vars, usages, truly_globals, dependency_stack);
            }
            if let Some(cond) = cond {
                check_expression_for_locality(cond, global_vars, usages, truly_globals, dependency_stack);
            }
            if let Some(stmt) = stmt2 {
                check_statement_for_locality(stmt, global_vars, usages, truly_globals, dependency_stack);
            }
            dependency_stack.push(HashMap::new()); //add new branch scope
            check_statement_for_locality(stmt3, global_vars, usages, truly_globals, dependency_stack);
            dependency_stack.pop();
        },
        Stmt::ForEach(_, expr, stmt) => {
            check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack);
            dependency_stack.push(HashMap::new()); //add new branch scope
            check_statement_for_locality(stmt, global_vars, usages, truly_globals, dependency_stack);
            dependency_stack.pop();
        }
        Stmt::Print(expressions, spec) => {  // file spec is not implemented here
            if let Some(_) = spec {
                panic!("Not implemented exception in check statement parallelizability: print with file spec");
            }
            for expr in expressions.iter() {
                check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack);
            }
        }
        _ => panic!("Not implemented exception in local detection for statement")
    }
}

fn check_expression_for_locality<'a, I: Clone + Hash + Eq+Debug>(expr: &Expr<'_, 'a, I>, global_vars: &HashSet<GlobalVar<'a, I>>, usages: &mut HashMap<GlobalVar<'a, I>, HashSet<GlobalVar<'a, I>>>, truly_globals: &mut HashSet<GlobalVar<'a, I>>, dependency_stack: &mut Vec<HashMap<GlobalVar<'a, I>, HashSet<GlobalVar<'a, I>>>>) -> HashSet<GlobalVar<'a, I>> {
    match expr {
        Expr::ILit(..) => HashSet::new(),
        Expr::FLit(..) => HashSet::new(),
        Expr::StrLit(..) => HashSet::new(),
        Expr::PatLit(..) => HashSet::new(),
        Expr::Unop(_, var) => check_expression_for_locality(var, global_vars, usages, truly_globals, dependency_stack),
        Expr::Binop(_, var1, var2)
        | Expr::And(var1, var2)
        | Expr::Or(var1, var2) => {
            let mut dependencies_in_var1 = check_expression_for_locality(var1, global_vars, usages, truly_globals, dependency_stack);
            dependencies_in_var1.extend(check_expression_for_locality(var2, global_vars, usages, truly_globals, dependency_stack));
            dependencies_in_var1
        },
        Expr::Var(var) => {
            let gl_var = GlobalVar::Scalar(var.clone());
            if global_vars.contains(&gl_var){
                check_if_assigned(&gl_var, truly_globals, dependency_stack);
                HashSet::from([gl_var])
            } else {HashSet::new()}
        },
        Expr::Index(var, ind) => {
            let mut dependencies = check_expression_for_locality(var, global_vars, usages, truly_globals, dependency_stack);
            dependencies.extend(check_expression_for_locality(ind, global_vars, usages, truly_globals, dependency_stack));
            let gl_var = index_into_gl_var(var, ind, global_vars);
            match gl_var {
                None => HashSet::new(),
                Some(a@ GlobalVar::ArrayUnknown(_)) => {
                    truly_globals.insert(a.clone());
                    HashSet::from([a])
                }
                Some(arr_ind) => {
                    check_if_assigned(&arr_ind, truly_globals, dependency_stack);
                    HashSet::from([arr_ind])
                }
            }
        }
        Expr::Assign(lhs, expr) => {
            if !valid_lhs(lhs) {
                return HashSet::new();
            }
            match lhs {
                Expr::Var(var) => {
                    let gl_var = GlobalVar::Scalar(var.clone());
                    let mut expr_dependencies = check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack);
                    if !global_vars.contains(&gl_var) {expr_dependencies}
                    else {
                        add_dependency(&gl_var, &expr_dependencies, usages, truly_globals, dependency_stack);
                        expr_dependencies.insert(gl_var);
                        expr_dependencies
                    }

                },
                Expr::Unop(ast::Unop::Column, expr) => {
                    let mut dependencies_in_var1 = check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack);
                    dependencies_in_var1.extend(check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack));
                    dependencies_in_var1
                },
                a @ Expr::Index(var, ind) => {
                    let mut dependencies = check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack); //check for expressions in rhs
                    let gl_var =   index_into_gl_var(var, ind, global_vars);
                    match gl_var {
                        None => dependencies,
                        Some(GlobalVar::ArrayUnknown(_)) => {
                            dependencies.extend(check_expression_for_locality(a, global_vars, usages, truly_globals, dependency_stack));
                            dependencies
                        }
                        Some(val) => {
                            add_dependency(&val, &dependencies, usages, truly_globals, dependency_stack);
                            dependencies.insert(val);
                            dependencies
                        }
                    }
                },
                _ => panic!("Unreachable statement in check_parallelization AssignOp")
            }
        },
        Expr::AssignOp(lhs, _, expr) => {
            if !valid_lhs(lhs) {
                return HashSet::new();
            }
            match lhs {
                Expr::Var(var) => {
                    let gl_var = GlobalVar::Scalar(var.clone());
                    let mut expr_dependencies = check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack);
                    if !global_vars.contains(&gl_var) {expr_dependencies}
                    else {
                        expr_dependencies.insert(gl_var.clone());
                        add_dependency(&gl_var, &expr_dependencies, usages, truly_globals, dependency_stack);
                        expr_dependencies
                    }

                },
                Expr::Unop(ast::Unop::Column, expr) => {
                    let mut dependencies_in_var1 = check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack);
                    dependencies_in_var1.extend(check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack));
                    dependencies_in_var1
                },
                a @ Expr::Index(var, ind) => {
                    let mut dependencies = check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack); //check for expressions in rhs
                    let gl_var =   index_into_gl_var(var, ind, global_vars);
                    match gl_var {
                        None => dependencies,
                        Some(GlobalVar::ArrayUnknown(_)) => {
                            dependencies.extend(check_expression_for_locality(a, global_vars, usages, truly_globals, dependency_stack));
                            dependencies
                        }
                        Some(val) => {
                            dependencies.insert(val.clone());
                            add_dependency(&val, &dependencies, usages, truly_globals, dependency_stack);
                            dependencies.insert(val);
                            dependencies
                        }
                    }
                },
                _ => panic!("Unreachable statement in check_parallelization AssignOp")
            }
        },
        Expr::Inc { is_inc, is_post, x } => {
            if !valid_lhs(x) {
                return HashSet::new();
            }
            match x {
                Expr::Var(var) => {
                    let gl_var = GlobalVar::Scalar(var.clone());
                    if !global_vars.contains(&gl_var) {HashSet::new()}
                    else {
                        let dependency = HashSet::from([gl_var.clone()]);
                        add_dependency(&gl_var, &dependency, usages, truly_globals, dependency_stack);
                        dependency
                    }

                },
                Expr::Unop(ast::Unop::Column, expr) => {
                    let mut dependencies_in_var1 = check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack);
                    dependencies_in_var1.extend(check_expression_for_locality(expr, global_vars, usages, truly_globals, dependency_stack));
                    dependencies_in_var1
                },
                a @ Expr::Index(var, ind) => {
                    let gl_var =   index_into_gl_var(var, ind, global_vars);
                    match gl_var {
                        None => HashSet::new(),
                        Some(GlobalVar::ArrayUnknown(_)) => {
                            check_expression_for_locality(a, global_vars, usages, truly_globals, dependency_stack)
                        }
                        Some(val) => {
                            let dependency = HashSet::from([val.clone()]);
                            add_dependency(&val, &dependency, usages, truly_globals, dependency_stack);
                            dependency
                        }
                    }
                },
                _ => panic!("Unreachable statement in check_parallelization AssignOp")
            }
        },
        Expr::ITE(var1, var2, var3) => {
            let mut res_dependencies = check_expression_for_locality(var1, global_vars, usages, truly_globals, dependency_stack);
            dependency_stack.push(HashMap::new()); //add new branch scope
            res_dependencies.extend(check_expression_for_locality(var2, global_vars, usages, truly_globals, dependency_stack));
            let assign_in_if = dependency_stack.pop().unwrap();
            dependency_stack.push(HashMap::new());
            res_dependencies.extend(check_expression_for_locality(var3, global_vars, usages, truly_globals, dependency_stack));
            let assign_in_else = dependency_stack.pop().unwrap();
            connect_environments(assign_in_if, assign_in_else, dependency_stack);
            res_dependencies

        },

        _ => panic!("Not implemented exception")
    }
}

fn check_if_assigned<'a, I: Clone + Hash + Eq+Debug>(var: &GlobalVar<'a, I>, truly_globals: &mut HashSet<GlobalVar<'a, I>>, dependency_stack: &mut Vec<HashMap<GlobalVar<'a, I>, HashSet<GlobalVar<'a, I>>>>) {
    if truly_globals.contains(var) {return}
    for l in dependency_stack {
        if l.contains_key(var) {return}
    }
    truly_globals.insert(var.clone());
}

fn add_dependency<'a, I: Clone + Hash + Eq+Debug>(var: &GlobalVar<'a, I>, expr_dependencies: &HashSet<GlobalVar<'a, I>>, usages: &mut HashMap<GlobalVar<'a, I>, HashSet<GlobalVar<'a, I>>>, truly_globals: &mut HashSet<GlobalVar<'a, I>>, dependency_stack: &mut Vec<HashMap<GlobalVar<'a, I>, HashSet<GlobalVar<'a, I>>>>) {
    modify_usages(expr_dependencies, var, usages);
    if truly_globals.contains(var) {return}  //assignment of already defined truly global
    if !expr_dependencies.contains(var) { //assignment independently of previous state
        dependency_stack.last_mut().unwrap().insert(var.clone(), expr_dependencies.clone());
        return
    }
    for l in dependency_stack.iter().rev() { //find assignment based on previously assigned value
        if l.contains_key(var) {
            let mut expr_dependencies = expr_dependencies.clone();
            expr_dependencies.remove(var);
            expr_dependencies.extend(l.get(var).unwrap().iter().cloned());
            dependency_stack.last_mut().unwrap().insert(var.clone(), expr_dependencies);
            return
        }
    }
    truly_globals.insert(var.clone()); //new assignment based on previous value
}

fn modify_usages<'a, I: Clone + Hash + Eq+Debug>(dependencies: &HashSet<GlobalVar<'a, I>>, var: &GlobalVar<'a, I>, usages: &mut HashMap<GlobalVar<'a, I>, HashSet<GlobalVar<'a, I>>>) {
    for f in dependencies {
        if !usages.contains_key(f) {usages.insert(f.clone(), HashSet::from([var.clone()]));}
        else {
            usages.get_mut(f).unwrap().insert(var.clone());
        }
    }
}

fn connect_environments<'a, I: Clone + Hash + Eq+Debug>(env1: HashMap<GlobalVar<'a, I>, HashSet<GlobalVar<'a, I>>>, env2: HashMap<GlobalVar<'a, I>, HashSet<GlobalVar<'a, I>>>, dependency_stack: &mut Vec<HashMap<GlobalVar<'a, I>, HashSet<GlobalVar<'a, I>>>>) {
    for k in env1.keys() {
        if !env2.contains_key(k) {continue}
        for l in dependency_stack.iter().rev() { //find assignment based on previously assigned value
            if l.contains_key(k) {
                let mut expr_dependencies = env1.get(k).unwrap().clone();
                expr_dependencies.extend(env2.get(k).unwrap().iter().cloned());
                expr_dependencies.extend(l.get(k).unwrap().iter().cloned());
                dependency_stack.last_mut().unwrap().insert(k.clone(), expr_dependencies);
                return
            }
        }
        let mut expr_dependencies = env1.get(k).unwrap().clone();
        expr_dependencies.extend(env2.get(k).unwrap().iter().cloned());
        dependency_stack.last_mut().unwrap().insert(k.clone(), expr_dependencies);
    }
}

pub fn index_into_gl_var<'a, I: Clone + Hash + Eq+Debug>(var: &Expr<I>, ind: &Expr<'_, 'a, I>, global_var: &HashSet<GlobalVar<'a, I>>) -> Option<GlobalVar<'a, I>> {
    match var {
        Expr::Var(val) => {
            let unknown = GlobalVar::ArrayUnknown(val.clone());
            if global_var.contains(&unknown) {return Some(unknown);}
            match ind {
                Expr::ILit(i) => {
                    let gl_var = GlobalVar::ArrayExact(val.clone(), IndexVal::IntLit(i.clone()));
                    if global_var.contains(&gl_var) {
                        return Some(gl_var);
                    }
                    None
                },
                Expr::StrLit(i) => {
                    let gl_var = GlobalVar::ArrayExact(val.clone(), IndexVal::StrLit(i.clone()));
                    if global_var.contains(&gl_var) {
                        return Some(gl_var);
                    }
                    None
                },
                Expr::PatLit(i) => {
                    let gl_var = GlobalVar::ArrayExact(val.clone(), IndexVal::PatLit(i.clone()));
                    if global_var.contains(&gl_var) {
                        return Some(gl_var);
                    }
                    None
                },
                _ => {
                    None
                }
            }
        }
        _ => {
            eprintln!("Unrecognized expression as array name in detect_locals");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::Arena;
    use crate::ast::{Expr, Stmt, Binop, Unop, Prog, Pattern};
    use std::collections::{HashSet, HashMap};

    fn mk_prog<'a, 'b, I: Clone + Hash + Eq + Debug>(
        arena: &'a Arena, pats: Option<&'b Stmt<I>>
    ) -> Prog<'a, 'b, I> {
        let stmt = Arena::new_vec_from_slice(arena, &[(Pattern::Null, pats)]);
        Prog { field_sep: None, prelude_vardecs: vec![], output_sep: None, output_record_sep: None,
            decs: Arena::new_vec(arena), begin: Arena::new_vec(arena), prepare: Arena::new_vec(arena),
            end: Arena::new_vec(arena), pats: stmt, stage: Default::default(), argv: vec![], parse_header: false }
    }

    #[test]
    fn test_no_global_usage() {
        // No global vars used
        let arena = Arena::default();
        let stmt = Stmt::Expr(&Expr::<&str>::ILit(5));

        let program = mk_prog(&arena, Some(&stmt));

        let globals = [].into();
        let res = find_truly_globals(&program, &globals);

        assert!(res.is_empty());
    }

    #[test]
    fn test_simple_usage() {
        // x = 1  ⇒ x becomes local
        let arena = Arena::default();
        let stmt = Stmt::Expr(&Expr::Var("x"));
        let program = mk_prog(&arena, Some(&stmt));

        let globals = [GlobalVar::Scalar("x")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [GlobalVar::Scalar("x")].into();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_simple_assignment() {
        // x = 1  ⇒ x becomes local
        let arena = Arena::default();
        let stmt = Stmt::Expr(&Expr::Assign(&Expr::Var("x"), &Expr::ILit(1)));
        let program = mk_prog(&arena, Some(&stmt));

        let globals = [GlobalVar::Scalar("x")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [].into();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_op_assignment() {
        // x += 1  ⇒ x becomes truly global
        let arena = Arena::default();
        let stmt = Stmt::Expr(&Expr::AssignOp(&Expr::Var("x"), Binop::Plus, &Expr::ILit(1)));
        let program = mk_prog(&arena, Some(&stmt));

        let globals = [GlobalVar::Scalar("x")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [GlobalVar::Scalar("x")].into();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_global_depends_on_global() {
        // x = y
        // y is global, so x becomes truly global AND depends on y
        let arena = Arena::default();
        let stmt = Stmt::Expr(&Expr::Assign(&Expr::Var("x"), &Expr::Var("y")));
        let program = mk_prog(&arena, Some(&stmt));

        let globals = [GlobalVar::Scalar("x"), GlobalVar::Scalar("y")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [GlobalVar::Scalar("x"), GlobalVar::Scalar("y")].into(); // y is not assigned, only read
        assert_eq!(res, expected);
    }

    #[test]
    fn test_block() {
        let arena = Arena::default();

        let stmt1 = Stmt::Expr(&Expr::Assign(&Expr::Var("x"), &Expr::ILit(1)));
        let stmt2 = Stmt::Expr(&Expr::Assign(&Expr::Var("y"), &Expr::ILit(2)));
        let stmt3 = Stmt::Expr(&Expr::AssignOp(&Expr::Var("z"), Binop::Plus, &Expr::ILit(1)));
        let block = Stmt::Block(Arena::new_vec_from_slice(&arena, &[&stmt1, &stmt2, &stmt3]));

        let program = mk_prog(&arena, Some(&block));

        let globals = [GlobalVar::Scalar("x"), GlobalVar::Scalar("y"), GlobalVar::Scalar("z")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [GlobalVar::Scalar("z")].into();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_chained_dependencies() {
        let arena = Arena::default();

        let stmt1 = Stmt::Expr(&Expr::Assign(&Expr::Var("x"), &Expr::ILit(1)));
        let stmt2 = Stmt::Expr(&Expr::Assign(&Expr::Var("y"), &Expr::ILit(2)));
        let stmt3 = Stmt::Expr(&Expr::AssignOp(&Expr::Var("z"), Binop::Plus, &Expr::Binop(Binop::Plus, &Expr::Var("x"), &Expr::Var("y"))));
        let block = Stmt::Block(Arena::new_vec_from_slice(&arena, &[&stmt1, &stmt2, &stmt3]));

        let program = mk_prog(&arena, Some(&block));

        let globals = [GlobalVar::Scalar("x"), GlobalVar::Scalar("y"), GlobalVar::Scalar("z")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [GlobalVar::Scalar("z")].into();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_if_branch_assignments() {
        // if (cond) x = y else x = z
        // y, z local => x is also local
        let arena = Arena::default();
        let stmt = Stmt::If(
            &Expr::Var("cond"),
            &Stmt::Expr(&Expr::Assign(&Expr::Var("x"), &Expr::Var("y"))),
            Some(&Stmt::Expr(&Expr::Assign(&Expr::Var("x"), &Expr::Var("z")))),
        );

        let program = mk_prog(&arena, Some(&stmt));

        let globals = [GlobalVar::Scalar("x")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [].into();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_if_without_else() {
        // if (cond) x = y
        // cond x assignment => x is not used anywhere else - local
        let arena = Arena::default();
        let stmt = Stmt::If(
            &Expr::Var("cond"),
            &Stmt::Expr(&Expr::Assign(&Expr::Var("x"), &Expr::Var("y"))),
            None,
        );

        let program = mk_prog(&arena, Some(&stmt));

        let globals= [GlobalVar::Scalar("x")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [].into();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_if_without_else_used() {
        // if (cond) x = y; use x
        // cond x assignment => x is global
        let arena = Arena::default();
        let stmt = Stmt::If(
            &Expr::Var("cond"),
            &Stmt::Expr(&Expr::Assign(&Expr::Var("x"), &Expr::Var("y"))),
            None,
        );
        let block = Stmt::Block(Arena::new_vec_from_slice(&arena, &[&stmt, &Stmt::Expr(&Expr::Var("x"))]));

        let program = mk_prog(&arena, Some(&block));

        let globals = [GlobalVar::Scalar("x")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [GlobalVar::Scalar("x")].into();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_inc_expression() {
        // y++    where y is global ⇒ y becomes truly global
        let arena = Arena::default();
        let stmt = Stmt::Expr(
            &Expr::Inc {
                is_inc: true,
                is_post: true,
                x: &Expr::Var("y"),
            }
        );

        let program = mk_prog(&arena, Some(&stmt));

        let globals = [GlobalVar::Scalar("y")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [GlobalVar::Scalar("y")].into();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_ite_expression() {
        // x = (cond ? y : z)
        let arena = Arena::default();
        let stmt = Stmt::Expr(
            &Expr::Assign(
                &Expr::Var("x"),
                &Expr::ITE(
                    &Expr::Var("cond"),
                    &Expr::Var("y"),
                    &Expr::Var("z"),
                ),
            )
        );

        let program = mk_prog(&arena, Some(&stmt));

        let globals = [GlobalVar::Scalar("x")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [].into();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_ite_with_reassignment() {
        // x = (cond ? y : z)
        let arena = Arena::default();
        let stmt = Stmt::Expr(
            &Expr::Assign(
                &Expr::Var("x"),
                &Expr::ITE(
                    &Expr::Var("cond"),
                    &Expr::Var("x"),
                    &Expr::Var("z"),
                ),
            )
        );

        let program = mk_prog(&arena, Some(&stmt));

        let globals = [GlobalVar::Scalar("x")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [GlobalVar::Scalar("x")].into();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_nested_blocks() {
        // { { x = y } }
        let arena = Arena::default();
        let inner = Stmt::Block(Arena::new_vec_from_slice(&arena, &[
            &Stmt::Expr(&Expr::Assign(&Expr::Var("x"), &Expr::Var("y")))
        ]));
        let outer = Stmt::Block(Arena::new_vec_from_slice(&arena, &[&inner]));

        let program = mk_prog(&arena, Some(&outer));

        let globals = [GlobalVar::Scalar("x"), GlobalVar::Scalar("y")].into();
        let res = find_truly_globals(&program, &globals);

        let expected = [GlobalVar::Scalar("x"), GlobalVar::Scalar("y")].into();
        assert_eq!(res, expected);
    }
}
