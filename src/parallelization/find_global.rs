use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::fmt::Debug;
use std::hash::Hash;
use builtins::Variable;
use crate::{ast, builtins};
use crate::ast::{Expr, Pattern, Stmt};

#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub enum GlobalVar<'a, I: Clone + Hash + Eq + Debug> {
    Scalar(I),
    ArrayExact(I, IndexVal<'a>),   // array with index known statically
    ArrayUnknown(I),               // array accessed with a dynamic index
}

#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub enum IndexVal<'a> {
    IntLit(i64),
    StrLit(&'a [u8]),
    PatLit(&'a [u8])
}

#[derive(Clone, Hash, Eq, PartialEq, Debug)]
enum ArrType {
    Int,
    Str,
    Pat,
    Unknown
}

/// Returns the global variables (the ones that have at least one assignment in the main loop).
pub fn find_global<'a, I: Clone + Hash + Eq + Debug>(program: &ast::Prog<'_, 'a, I>) -> (bool, HashSet<GlobalVar<'a, I>>)
where
    Variable: TryFrom<I>
{
    let mut res = HashSet::new();
    let mut array_type: HashMap<I, ArrType> = HashMap::new();
    for (pattern, maybe_stmt) in &program.pats {
        let val = has_assigns_in_pattern(pattern, &mut array_type);
        if !val.0 {return (false, HashSet::new())}
        res.extend(val.1);
        match maybe_stmt {
            Some(stmt) => {
                let val = has_assigns_in_stmt(stmt, &mut array_type);
                if !val.0 {return (false, HashSet::new())}
                res.extend(val.1)
            },
            None => continue,
        }
    }
    let mut no_builtin_change = true;
    let res = res.into_iter().map(|x| {
        match x {
            GlobalVar::Scalar(i) => {
                if let Ok(_) = Variable::try_from(i.clone()) {no_builtin_change = false;}
                GlobalVar::Scalar(i)
            },
            a @ GlobalVar::ArrayUnknown(_) => a,
            GlobalVar::ArrayExact(i, val) => {
                if arr_type_sim(&val, array_type.get(&i).unwrap()) {
                    GlobalVar::ArrayExact(i, val)
                } else {
                    GlobalVar::ArrayUnknown(i)
                }
            }
        }
    }).collect();
    (no_builtin_change, res)
}

fn arr_type_sim(i: &IndexVal, val: &ArrType) -> bool {
    match i {
        IndexVal::IntLit(_) => *val == ArrType::Int,
        IndexVal::StrLit(_) => *val == ArrType::Str,
        IndexVal::PatLit(_) => *val == ArrType::Pat,
    }
}

fn has_assigns_in_pattern<'a, I: Clone + Hash + Eq+Debug>(pattern: &Pattern<'_, 'a, I>, array_type: &mut HashMap<I, ArrType>) -> (bool, HashSet<GlobalVar<'a, I>>) where builtins::Variable: TryFrom<I> {
    match pattern {
        Pattern::Null => (true, HashSet::new()),
        Pattern::Comma(e1, e2) => (false, HashSet::new()),
        Pattern::Bool(e) => has_assigns_in_expr(e, array_type)
    }
}

/// Returns any variable assignments in the main loop
fn has_assigns_in_stmt<'a, I: Clone + Hash + Eq + Debug>(stmt: &Stmt<'_, 'a, I>, array_type: &mut HashMap<I, ArrType>) -> (bool, HashSet<GlobalVar<'a, I>>) where builtins::Variable: TryFrom<I> {
    match stmt {
        Stmt::Break | Stmt::Continue | Stmt::Next | Stmt::NextFile => (true, HashSet::new()),
        Stmt::StartCond(..) | Stmt::EndCond(..) | Stmt::LastCond(..) => (true, HashSet::new()),
        Stmt::Expr(v) => has_assigns_in_expr(v, array_type),

        Stmt::Block(stmts) => {
            let mut out = HashSet::new();
            for s in stmts {
                let res = has_assigns_in_stmt(s, array_type);
                if !res.0 {return (false, HashSet::new())}
                out.extend(res.1)
            }
            (true, out)
        }

        Stmt::If(cond, t, f) => { //needs to be fixed
            let mut out = has_assigns_in_expr(cond, array_type);
            if !out.0 {return (false, HashSet::new())}
            let res1 = has_assigns_in_stmt(t, array_type);
            if !res1.0 {return (false, HashSet::new())}
            connect_two_hashsets(&mut out, res1);
            if let Some(f) = f {
                let res2 = has_assigns_in_stmt(f, array_type);
                if !res2.0 {return (false, HashSet::new())}
                connect_two_hashsets(&mut out, res2);
            }
            out
        }

        Stmt::While(_, cond, body)  //needs to be fixed
        | Stmt::DoWhile(cond, body)
        | Stmt::ForEach(_, cond, body)
        => {
            let mut out = has_assigns_in_expr(cond, array_type);
            let res = has_assigns_in_stmt(body, array_type);
            connect_two_hashsets(&mut out, res);
            out
        },

        Stmt::For(stmt1, expr, stmt3, body) => {
            let mut res = has_assigns_in_stmt(body, array_type);
            for opt in [stmt1, stmt3] {
                if let Some(stmt) = opt {
                    let stmt1_as = has_assigns_in_stmt(stmt, array_type);
                    connect_two_hashsets(&mut res, stmt1_as);
                }
            }
            if let Some(expr) = expr {
                let expr_as= has_assigns_in_expr(expr, array_type);
                connect_two_hashsets(&mut res, expr_as);
            }
            res
        },

        Stmt::Print(vec, optexpr) => {
            let mut res = (true, HashSet::new());
            for val in vec.iter() {
                connect_two_hashsets(&mut res, has_assigns_in_expr(val, array_type));
            }
            if let Some((expr, _)) = optexpr {
                connect_two_hashsets(&mut res, has_assigns_in_expr(expr, array_type));
            }
            res
        }

        _ => panic!("Not implemented exception in find global statement: {:?}", stmt),
    }
}

fn has_assigns_in_expr<'a, I: Clone + Hash + Eq + Debug>(expr: &Expr<'_, 'a, I>, array_type: &mut HashMap<I, ArrType>) -> (bool, HashSet<GlobalVar<'a, I>>) where  Variable: TryFrom<I> {
    match expr {
        Expr::ILit(..) | Expr::FLit(..) | Expr::PatLit(..) | Expr::StrLit(..) |
        Expr::ReadStdin | Expr::Cond(..) => (true, HashSet::new()),
        Expr::Var(v) => {
            if let Ok(v) = Variable::try_from(v.clone()) {
                return match v {
                    Variable::NR | Variable::FNR => { (false, HashSet::new()) },
                    _ => { (true, HashSet::new()) }
                }
            }
            (true, HashSet::new())
        },

        Expr::Unop(_, expr) => has_assigns_in_expr(expr, array_type),
        Expr::Binop(_, expr1, expr2)
        | Expr::And(expr1, expr2)
        | Expr::Or(expr1, expr2) => {
            let mut res = has_assigns_in_expr(expr1, array_type);
            let res2 = has_assigns_in_expr(expr2, array_type);
            connect_two_hashsets(&mut res, res2);
            res
        },
        Expr::Assign(lhs, rhs)
        | Expr::AssignOp(lhs, _, rhs) => {
            let res1 = has_assigns_in_expr(lhs, array_type);
            let mut res2 = has_assigns_in_expr(rhs, array_type);
            match lhs {
                Expr::Var(v) => {
                    res2.1.insert(GlobalVar::Scalar(v.clone()));
                }
                Expr::Index(v, ind) => process_index(v, ind, &mut res2.1, array_type),
                _ => {}
            }
            connect_two_hashsets(&mut res2, res1);
            res2
        },
        Expr::Inc { is_inc: _is_inc, is_post: _is_post, x} => {
            let mut res = has_assigns_in_expr(x, array_type);
            if let Expr::Var(v) = x {
                res.1.insert(GlobalVar::Scalar(v.clone()));
                res
            } else {
                res
            }
        }
        Expr::ITE(cond, expr1, expr2) => {
            let mut res = has_assigns_in_expr(cond, array_type);
            let res2 = has_assigns_in_expr(expr1, array_type);
            connect_two_hashsets(&mut res, res2);
            let res3 = has_assigns_in_expr(expr2, array_type);
            connect_two_hashsets(&mut res, res3);
            res
        },
        Expr::Index(expr1, expr2) => {
            let mut res = has_assigns_in_expr(expr1, array_type);
            let res2 = has_assigns_in_expr(expr2, array_type);
            connect_two_hashsets(&mut res, res2);
            match expr1 {
                Expr::Var(v) => {
                    match expr2 {
                        Expr::ILit(_) => {
                            check_arr_type(v, array_type, ArrType::Int);
                        },
                        Expr::StrLit(_) => {
                            check_arr_type(v, array_type, ArrType::Str);
                        },
                        Expr::PatLit(_) => {
                            check_arr_type(v, array_type, ArrType::Pat);
                        },
                        _ => {
                            if array_type.contains_key(v) {
                                if *array_type.get(v).unwrap() != ArrType::Unknown {
                                    array_type.insert(v.clone(), ArrType::Unknown);
                                }
                            } else {
                                array_type.insert(v.clone(), ArrType::Unknown);
                            }

                        }
                    }
                }
                _ => {}
            }
            res
        }
        _ => panic!("Not implemented exception in find global expression: {:?}", expr),
    }
}

fn process_index<'a, I: Clone + Hash + Eq + Debug>(name: &Expr<I>, index: &Expr<'_, 'a, I>, res: &mut HashSet<GlobalVar<'a, I>>, array_type: &mut HashMap<I, ArrType>) {
    match name {
        Expr::Var(v) => {
            match index {
                Expr::ILit(i) => {
                    if !check_arr_type(v, array_type, ArrType::Int) {
                        res.insert(GlobalVar::ArrayUnknown(v.clone()));
                        return
                    }
                    res.insert(GlobalVar::ArrayExact(v.clone(), IndexVal::IntLit(i.clone())));
                },
                Expr::StrLit(i) => {
                    if !check_arr_type(v, array_type, ArrType::Str) {
                        res.insert(GlobalVar::ArrayUnknown(v.clone()));
                        return
                    }
                    res.insert(GlobalVar::ArrayExact(v.clone(), IndexVal::StrLit(i.clone())));
                },
                Expr::PatLit(i) => {
                    if !check_arr_type(v, array_type, ArrType::Pat) {
                        res.insert(GlobalVar::ArrayUnknown(v.clone()));
                        return
                    }
                    res.insert(GlobalVar::ArrayExact(v.clone(), IndexVal::PatLit(i.clone())));
                },
                _ => {
                    array_type.insert(v.clone(), ArrType::Unknown);
                    res.insert(GlobalVar::ArrayUnknown(v.clone()));
                }
            }
        }
        _ => {
            eprintln!("Unrecognized array name expression: {:?}", name);
            return
        }
    }
}

fn check_arr_type<I: Clone + Hash + Eq + Debug>(v: &I, array_type: &mut HashMap<I, ArrType>, desired_type: ArrType) -> bool {
    if array_type.contains_key(v) {
        if *array_type.get(v).unwrap() == desired_type {
            true
        } else {
            array_type.insert(v.clone(), ArrType::Unknown);
            false
        }
    } else {
        array_type.insert(v.clone(), desired_type);
        true
    }
}

fn connect_two_hashsets<'a, I: Clone + Hash + Eq + Debug>(res1: &mut (bool, HashSet<GlobalVar<'a, I>>), res2: (bool, HashSet<GlobalVar<'a, I>>)) {
    res1.0  = res1.0 && res2.0;
    if !res1.0 {return}
    res1.1.extend(res2.1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Binop, Expr, Pattern, Stmt, Unop};
    use crate::arena::Arena;

    #[test]
    fn test_find_basic_pattern() {
        let pattern = Pattern::Bool(&Expr::Assign(&Expr::Var("x"), &Expr::Var("z")));
        let globals = has_assigns_in_pattern(&pattern, &mut HashMap::new());

        let expected = (true, [GlobalVar::Scalar("x")].into());
        assert_eq!(globals, expected);
    }

    #[test]
    fn test_find_basic_pattern_twice() {
        let pattern = Pattern::Bool(&Expr::Assign(&Expr::Var("x"), &Expr::Assign(&Expr::Var("x"), &Expr::Var("z"))));
        let globals = has_assigns_in_pattern(&pattern, &mut HashMap::new());

        let expected = (true, [GlobalVar::Scalar("x")].into());
        assert_eq!(globals, expected);
    }

    #[test]
    fn test_find_null_pattern() {
        let pattern: Pattern<'_, '_, &str> = Pattern::Null;
        let globals = has_assigns_in_pattern(&pattern, &mut HashMap::new());

        let expected= (true, [].into());
        assert_eq!(globals, expected);
    }

    #[test]
    fn test_find_comma_pattern() {
        let pattern = Pattern::Comma(&Expr::Assign(&Expr::Var("x"), &Expr::Var("z")), &Expr::AssignOp(&Expr::Var("d"), Binop::Div, &Expr::Var("z")));
        let globals = has_assigns_in_pattern(&pattern, &mut HashMap::new());

        let expected = (false, [].into());
        assert_eq!(globals, expected);
    }

    #[test]
    fn test_find_expression() {
        let stmt = Stmt::Expr(&Expr::Assign(&Expr::Var("x"),
                                            &Expr::ITE(
                                                &Expr::Inc {is_inc:true, is_post:true, x:&Expr::Var("y")},
                                                &Expr::Assign(&Expr::Var("z"), &Expr::Var("d")),
                                                &Expr::AssignOp(&Expr::Unop(Unop::Column, &Expr::Var("g")), Binop::Plus, &Expr::Var("l")))));
        let globals = has_assigns_in_stmt(&stmt, &mut HashMap::new());

        let expected = (true, [GlobalVar::Scalar("x"), GlobalVar::Scalar("y"),
            GlobalVar::Scalar("z")].into());
        assert_eq!(globals, expected);
    }

    #[test]
    fn test_find_statement() {
        let arena = Arena::default();

        let vec = Arena::new_vec_from_slice(&arena, &[&Stmt::Continue,
            &Stmt::If(&Expr::Assign(&Expr::Var("x"), &Expr::ILit(1)),
                      &Stmt::Expr(&Expr::Assign(&Expr::Var("y"), &Expr::ILit(1))), Some(&Stmt::Expr(&Expr::Assign(&Expr::Var("z"), &Expr::ILit(1)))))]);
        let stmt = Stmt::Block(vec);
        let globals = has_assigns_in_stmt(&stmt, &mut HashMap::new());

        let expected = (true, [GlobalVar::Scalar("x"), GlobalVar::Scalar("y"),
            GlobalVar::Scalar("z")].into());
        assert_eq!(globals, expected);
    }

    #[test]
    fn test_find_statement_print() {
        let stmt = Stmt::Print(&[&Expr::Assign(&Expr::Var("x"), &Expr::ILit(1)),
                      &Expr::Assign(&Expr::Var("y"), &Expr::ILit(1)), &Expr::Assign(&Expr::Var("z"), &Expr::ILit(1))], None);
        let globals = has_assigns_in_stmt(&stmt, &mut HashMap::new());

        let expected = (true, [GlobalVar::Scalar("x"), GlobalVar::Scalar("y"),
            GlobalVar::Scalar("z")].into());
        assert_eq!(globals, expected);
    }

    #[test]
    fn test_array_exact_index() {
        let stmt = Stmt::Expr(&Expr::Assign(
            &Expr::Index(&Expr::Var("arr"), &Expr::ILit(10)),
            &Expr::ILit(5)
        ));

        let globals = has_assigns_in_stmt(&stmt, &mut HashMap::new());

        let expected = (true, [GlobalVar::ArrayExact("arr", IndexVal::IntLit(10))].into());
        assert_eq!(globals, expected);
    }


    #[test]
    fn test_array_unknown_index() {
        let stmt = Stmt::Expr(&Expr::Assign(
            &Expr::Index(&Expr::Var("arr"), &Expr::Var("i")),
            &Expr::ILit(3)
        ));

        let globals = has_assigns_in_stmt(&stmt, &mut HashMap::new());

        let expected = (true, [GlobalVar::ArrayUnknown("arr")].into());
        assert_eq!(globals, expected);
    }


    #[test]
    fn test_array_exact_str_index() {
        let index = b"name";
        let stmt = Stmt::Expr(&Expr::Assign(
            &Expr::Index(&Expr::Var("map"), &Expr::StrLit(index)),
            &Expr::ILit(1),
        ));

        let globals = has_assigns_in_stmt(&stmt, &mut HashMap::new());

        let expected = (true, [GlobalVar::ArrayExact("map", IndexVal::StrLit(index))].into());
        assert_eq!(globals, expected);
    }


    #[test]
    fn test_nested_while_assign_arrays() {
        let stmt = Stmt::While(false,
                               &Expr::Var("c"),
                               &Stmt::While(false,
                                            &Expr::Var("d"),
                                            &Stmt::Expr(&Expr::Assign(
                                                &Expr::Index(&Expr::Var("arr"), &Expr::ILit(2)),
                                                &Expr::Var("z"),
                                            ))
                               )
        );

        let globals = has_assigns_in_stmt(&stmt, &mut HashMap::new());

        let expected = (true, [GlobalVar::ArrayExact("arr", IndexVal::IntLit(2))].into());
        assert_eq!(globals, expected);
    }


    #[test]
    fn test_for_loop_array_assign() {
        let stmt = Stmt::For(
            None,
            Some(&Expr::Var("cond")),
            None,
            &Stmt::Expr(&Expr::Assign(
                &Expr::Index(&Expr::Var("arr"), &Expr::Var("i")),
                &Expr::Var("y"),
            ))
        );

        let globals = has_assigns_in_stmt(&stmt, &mut HashMap::new());

        let expected = (true, [GlobalVar::ArrayUnknown("arr")].into());
        assert_eq!(globals, expected);
    }


    #[test]
    fn test_for_each_array_assign_with_exact_index() {
        let stmt = Stmt::ForEach(
            "v",
            &Expr::Var("cond"),
            &Stmt::Expr(&Expr::Assign(
                &Expr::Index(&Expr::Var("arr"), &Expr::ILit(7)),
                &Expr::ILit(4),
            ))
        );

        let globals = has_assigns_in_stmt(&stmt, &mut HashMap::new());

        let expected = (true, [GlobalVar::ArrayExact("arr", IndexVal::IntLit(7))].into());
        assert_eq!(globals, expected);
    }


    #[test]
    fn test_array_exact_then_unknown_index_combined() {
        let arena = Arena::default();
        let stmt = Stmt::Block(Arena::new_vec_from_slice(
            &arena,
            &[
                &Stmt::Expr(&Expr::Assign(
                    &Expr::Index(&Expr::Var("arr"), &Expr::ILit(1)),
                    &Expr::ILit(2)
                )),
                &Stmt::Expr(&Expr::Assign(
                    &Expr::Index(&Expr::Var("arr"), &Expr::Var("x")),
                    &Expr::ILit(3)
                ))
            ]
        ));
        let vec = Arena::new_vec_from_slice(&arena, &[(Pattern::Null, Some(&stmt))]);

        let prog = ast::Prog {
            field_sep: None,
            prelude_vardecs: vec![],
            output_sep: None,
            output_record_sep: None,
            pats: vec, //this field is only important
            stage: Default::default(),
            argv: vec![],
            begin: Arena::new_vec(&arena),
            prepare: Arena::new_vec(&arena),
            end: Arena::new_vec(&arena),
            decs: Arena::new_vec(&arena),
            parse_header: false,
        };

        let globals = find_global(&prog);

        // Second assignment forces downgrade to ArrayUnknown
        let expected = [GlobalVar::ArrayUnknown("arr")].into();
        assert_eq!(globals, (true, expected));
    }


    #[test]
    fn test_mixed_array_types_same_name() {
        let arena = Arena::default();
        let stmt = Stmt::Block(Arena::new_vec_from_slice(
            &arena,
            &[
                &Stmt::Expr(&Expr::Assign(
                    &Expr::Index(&Expr::Var("m"), &Expr::ILit(1)),
                    &Expr::ILit(10),
                )),
                &Stmt::Expr(&Expr::Assign(
                    &Expr::Index(&Expr::Var("m"), &Expr::StrLit(b"abc")),
                    &Expr::ILit(20),
                ))
            ]
        ));

        let vec = Arena::new_vec_from_slice(&arena, &[(Pattern::Null, Some(&stmt))]);

        let prog = ast::Prog {
            field_sep: None,
            prelude_vardecs: vec![],
            output_sep: None,
            output_record_sep: None,
            pats: vec, //this field is only important
            stage: Default::default(),
            argv: vec![],
            begin: Arena::new_vec(&arena),
            prepare: Arena::new_vec(&arena),
            end: Arena::new_vec(&arena),
            decs: Arena::new_vec(&arena),
            parse_header: false,
        };

        let globals = find_global(&prog);

        let expected = [GlobalVar::ArrayUnknown("m")].into();
        assert_eq!(globals, (true, expected));
    }

}
