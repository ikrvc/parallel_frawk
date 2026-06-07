use std::convert::TryFrom;
use hashbrown::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;
use crate::arena::Arena;
use crate::{ast, builtins};
use crate::ast::{Binop, Expr, Pattern, Stmt};
use crate::builtins::{Function, IsSprintf};
use crate::common::Either;
use crate::parallelization::detect_locals::{find_truly_globals, index_into_gl_var};
use crate::parallelization::find_global::{ArrayUnknown, GlobalVar};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ParallelOp {
    Plus,
    Mult,
    Concat,
    And,
    Or,
    LastAssigned
}

pub fn check_parallelizability<'a, 'b, I: IsSprintf+Clone + Hash + Eq+Debug>(program: &ast::Prog<'a, 'b, I>, global_vars: &HashSet<GlobalVar<'a, I>>, strict_output_order: bool) -> (bool, HashMap<GlobalVar<'a, I>, ParallelOp>)
    where Function: TryFrom<I>
{
    // println!("Global vars: {:?}", global_vars);
    let mut result: HashMap<GlobalVar<'a, I>, ParallelOp> = HashMap::new();
    
    for (pattern, _) in &program.pats {
        if !(check_pattern_parallelizability(pattern, global_vars)) {return (false, result)}
    }

    let truly_globals = find_truly_globals(program, global_vars);
    // println!("Truly globals: {:?}", truly_globals);

    for (_, maybe_stmt) in &program.pats {
        match maybe_stmt {
            Some(stmt) => {
                if !check_statement_parallelizability(stmt, &truly_globals, &mut result, strict_output_order) {return (false, result)}
            },
            None => continue,
        }
    }
    for var in global_vars {
        if !truly_globals.contains(var) {
            result.insert(var.clone(), ParallelOp::LastAssigned);
        }
    }
    // println!("Result: {:?}", result);
    (true, result)
}

fn check_pattern_parallelizability<'a, I: IsSprintf+ Clone + Hash + Eq+Debug>(pattern: &Pattern<I>, global_vars: &HashSet<GlobalVar<'a, I>>) -> bool
    where Function: TryFrom<I>
{
    match pattern {
        Pattern::Null => true,
        Pattern::Comma(..) => false,
        Pattern::Bool(e) => check_expression_for_global_vars(e, global_vars, &mut Vec::new())
    }
}

fn check_expression_for_global_vars<'a, 'b, I: IsSprintf+Clone + Hash + Eq+Debug>(expression: &Expr<I>, global_vars: &HashSet<GlobalVar<'b, I>>, not_allowed_assignments: &mut Vec<&'a Expr<'a, 'b, I>>) -> bool
    where Function: TryFrom<I>
{
    match expression {
        Expr::ILit(..) => true,
        Expr::FLit(..) => true,
        Expr::StrLit(..) => true,
        Expr::PatLit(..) => true,
        Expr::Unop(_, var) => check_expression_for_global_vars(var, global_vars, not_allowed_assignments),
        Expr::Binop(_, var1, var2) => {
            check_expression_for_global_vars(var1, global_vars, not_allowed_assignments) && check_expression_for_global_vars(var2, global_vars, not_allowed_assignments)
        },
        Expr::Var(var) => !global_vars.contains(&GlobalVar::Scalar(var.clone())),
        Expr::Assign(var1, var2)
        | Expr::AssignOp(var1, _, var2) => {
            if not_allowed_assignments.iter().any(|val| val.contains(var1)) {
                return false;
            }
            check_expression_for_global_vars(var1, global_vars, not_allowed_assignments) && check_expression_for_global_vars(var2, global_vars, not_allowed_assignments)
        },
        Expr::And(var1, var2)
        | Expr::Or(var1, var2) => {
            check_expression_for_global_vars(var1, global_vars, not_allowed_assignments) && check_expression_for_global_vars(var2, global_vars, not_allowed_assignments)
        },
        Expr::ITE(var1, var2, var3) => {
            check_expression_for_global_vars(var1, global_vars, not_allowed_assignments)
                && check_expression_for_global_vars(var2, global_vars, not_allowed_assignments)
                && check_expression_for_global_vars(var3, global_vars, not_allowed_assignments)
        },
        Expr::Inc {is_inc, is_post, x} => {
            if not_allowed_assignments.iter().any(|val| val.contains(x)) {
                return false;
            }
            check_expression_for_global_vars(x, global_vars, not_allowed_assignments)
        },
        Expr::Index(var, ind) => {
            let gl_var = index_into_gl_var(var, ind, global_vars);
            match gl_var {
                None => {
                    check_expression_for_global_vars(var, global_vars, not_allowed_assignments) && check_expression_for_global_vars(ind, global_vars, not_allowed_assignments)
                }
                Some(_) => false
            }
        }

        Expr::Cond(_) => false,

        //check what is this
        Expr::Getline { .. } => false,
        Expr::ReadStdin => false,

        Expr::Call(func, args) => {
            let func = define_builtin(func);
            match func {
                Either::Left(f) => {
                    if f.is_sprintf() {
                        let mut result = true;
                        for arg in args.iter() {
                            result = result && check_expression_for_global_vars(arg, global_vars, not_allowed_assignments);
                            if !result {return false}
                        }
                        result
                    } else { false }
                },
                Either::Right(val) => {
                    if !check_default_function_parallelizability(&val, args, global_vars) {return false}
                    let mut result = true;
                    for arg in args.iter() {
                        result = result && check_expression_for_global_vars(arg, global_vars, not_allowed_assignments);
                        if !result {return false}
                    }
                    result
                }
            }


        }
    }
}

pub fn define_builtin<I:Clone>(func: &Either<I, Function>) -> Either<I, Function>
    where Function: TryFrom<I>
{
    match func {
        Either::Left(val) => {
            if let Ok(f) = Function::try_from(val.clone()) {
                Either::Right(f)
            } else {
                Either::Left(val.clone())
            }
        }
        a @ Either::Right(_) => a.clone()
    }
}

fn check_statement_parallelizability<'a, I: IsSprintf+Clone + Hash + Eq+Debug>(stmt: &Stmt<'_, 'a, I>, global_vars: &HashSet<GlobalVar<'a, I>>, result: &mut HashMap<GlobalVar<'a, I>, ParallelOp>, strict_output_order: bool) -> bool
where Function: TryFrom<I>
{
    match stmt {
        Stmt::Expr(expr) => check_expression_for_parallelizability(expr, global_vars, result, &mut Vec::new()),
        Stmt::Block(vec) => {
            for stmt in vec {
                if !check_statement_parallelizability(stmt, global_vars, result, strict_output_order) {return false}
            }
            true
        }
        Stmt::If(cond, stmt1, stmt2) => {
            if !check_expression_for_global_vars(cond, global_vars, &mut Vec::new()) {return false}
            if !check_statement_parallelizability(stmt1, global_vars, result, strict_output_order) {return false}
            if let Some(stmt) = stmt2 {
                return check_statement_parallelizability(stmt, global_vars, result, strict_output_order)
            }
            true
        }
        Stmt::While(_, cond, stmt)
        | Stmt::ForEach(_, cond, stmt) => {
            match cond {
                Expr::Var(i) => { //special case when variable is actually an array
                    for var in global_vars {
                        match var {
                            a@ GlobalVar::Scalar(i_global)
                            | a @ GlobalVar::ArrayExact(i_global, _)
                            | a @ GlobalVar::ArrayUnknown(ArrayUnknown {id:i_global, fully_redefined:_})=> {
                                if i == i_global {return false}
                            }
                        }
                    }
                }
                _ => {if !check_expression_for_global_vars(cond, global_vars, &mut Vec::new()) {return false}}
            }
            if !check_statement_parallelizability(stmt, global_vars, result, strict_output_order) {return false}
            true
        },
        Stmt::DoWhile(cond, stmt) => {
            if !check_statement_parallelizability(stmt, global_vars, result, strict_output_order) {return false}
            if !check_expression_for_global_vars(cond, global_vars, &mut Vec::new()) {return false}
            true
        }
        Stmt::For(stmt1, cond, stmt2, stmt3) => {
            if let Some(stmt) = stmt1 {
                if !check_statement_parallelizability(stmt, global_vars, result, strict_output_order) {return false}
            }
            if let Some(cond) = cond {
                if !check_expression_for_global_vars(cond, global_vars, &mut Vec::new()) {return false}
            }
            if let Some(stmt) = stmt2 {
                if !check_statement_parallelizability(stmt, global_vars, result, strict_output_order) {return false}
            }
            check_statement_parallelizability(stmt3, global_vars, result, strict_output_order)
        }
        Stmt::Print(expressions, spec) => {  // file spec is not implemented here
            if strict_output_order {
                return false;
            }
            if let Some((expr, _)) = spec {
                if !check_expression_for_global_vars(expr, global_vars, &mut Vec::new()) {return false}
            }
            for expr in expressions.iter() {
                if !check_expression_for_global_vars(expr, global_vars, &mut Vec::new()) {return false}
            }
            true
        }
        Stmt::Printf(expr, variables, spec) => {  // file spec is not implemented here
            if strict_output_order {
                return false;
            }
            if let Some((e, _)) = spec {
                if !check_expression_for_global_vars(e, global_vars, &mut Vec::new()) {return false}
            }
            if !check_expression_for_global_vars(expr, global_vars, &mut Vec::new()) {return false}
            for expr in variables.iter() {
                if !check_expression_for_global_vars(expr, global_vars, &mut Vec::new()) {return false}
            }
            true
        }
        Stmt::Break | Stmt::Continue => true,
        Stmt::LastCond(_) | Stmt::EndCond(_) | Stmt::StartCond(_) => false,

        //check these later
        Stmt::Next | Stmt::NextFile | Stmt::Return(_) => false,
        // _ => panic!("Not implemented exception")
    }
}

fn check_expression_for_parallelizability<'a,'b, I: IsSprintf + Clone + Hash + Eq+Debug>
(expr: &Expr<'a, 'b, I>, global_vars: &HashSet<GlobalVar<'b, I>>, result: &mut HashMap<GlobalVar<'b, I>, ParallelOp>, not_allowed_assignments: &mut Vec<&'a Expr<'a, 'b, I>>) -> bool
where Function: TryFrom<I>
{
    match expr {
        Expr::ILit(..) => true,
        Expr::FLit(..) => true,
        Expr::StrLit(..) => true,
        Expr::PatLit(..) => true,
        Expr::Unop(_, var) => check_expression_for_parallelizability(var, global_vars, result, not_allowed_assignments),
        Expr::Binop(_, var1, var2) => {
            check_expression_for_parallelizability(var1, global_vars, result, not_allowed_assignments) && check_expression_for_parallelizability(var2, global_vars, result, not_allowed_assignments)
        },
        Expr::Var(_) => true, // check it closer
        Expr::Assign(lhs, expr) => {
            if !valid_lhs(lhs) {
                return false;
            }
            if not_allowed_assignments.iter().any(|val| val.contains(lhs)) {
                return false;
            }
            match lhs {
                Expr::Var(var) => {
                    let gl_var = GlobalVar::Scalar(var.clone());
                    if !global_vars.contains(&gl_var) {return check_expression_for_global_vars(expr, global_vars, not_allowed_assignments)}
                    if check_expression_for_global_vars(expr, global_vars, not_allowed_assignments) {return add_operator_for_global(gl_var, result, ParallelOp::LastAssigned)}
                    let oper_var = check_expression_under_assignment(&gl_var, expr, global_vars, result, None, not_allowed_assignments);
                    if oper_var.0 == false {return false}
                    match oper_var.1 {
                        None => {true}
                        Some(op) => add_operator_for_global(gl_var, result, op)
                    }
                },
                Expr::Unop(ast::Unop::Column, expr) => check_expression_for_global_vars(expr, global_vars, not_allowed_assignments),
                Expr::Index(var, ind) => {
                    if !check_expression_for_global_vars(ind, global_vars, not_allowed_assignments) {return false;} // in case rhs or ind contains global variable the expression is not parallelizable
                    let gl_var = index_into_gl_var(var, ind, global_vars);
                    match gl_var {
                        None => true,
                        // Some(a@ GlobalVar::ArrayUnknown(_)) => {
                        //     if check_expression_for_global_vars(expr, global_vars) {return add_operator_for_global(a, result, ParallelOp::LastAssigned)}
                        //     else {false} // this one should be adjusted to match in case of the same index
                        // }
                        Some(val ) => {
                            if check_expression_for_global_vars(expr, global_vars, not_allowed_assignments) {return add_operator_for_global(val, result, ParallelOp::LastAssigned)}
                            not_allowed_assignments.push(ind);
                            let oper_var = check_expression_under_assignment(&val, expr, global_vars, result, Some(ind), not_allowed_assignments);
                            if oper_var.0 == false {return false}
                            match oper_var.1 {
                                None => {true}
                                Some(op) => add_operator_for_global(val, result, op)
                            }
                        }
                    }
                },
                _ => panic!("Unreachable statement in check_parallelization AssignOp")
            }
        },
        a@ Expr::AssignOp(lhs, _, var2) => {
            if !valid_lhs(lhs) {
                return false;
            }
            if not_allowed_assignments.iter().any(|val| val.contains(lhs)) {
                return false;
            }
            match lhs {
                Expr::Var(var) => {
                    let gl_var = GlobalVar::Scalar(var.clone());
                    if !global_vars.contains(&gl_var) {return true}
                    if !check_expression_for_global_vars(var2, global_vars, not_allowed_assignments) {return false}
                    let op =  match find_par_operator(a) {
                        Some(op) => op,
                        None => return false
                    };
                    add_operator_for_global(gl_var, result, op)
                },
                Expr::Unop(ast::Unop::Column, expr) => check_expression_for_global_vars(expr, global_vars, not_allowed_assignments),
                Expr::Index(var, ind) => {
                    if !check_expression_for_global_vars(var2, global_vars, not_allowed_assignments) || !check_expression_for_global_vars(ind, global_vars, not_allowed_assignments) {return false;} // in case rhs or ind contains global variable the expression is not parallelizable
                    let gl_var = index_into_gl_var(var, ind, global_vars);
                    match gl_var {
                        None => true,
                        Some(val ) => {
                            let op = match find_par_operator(a) {
                                Some(op) => op,
                                None => return false
                            };
                            add_operator_for_global(val, result, op)
                        }
                    }
                },
                _ => panic!("Unreachable statement in check_parallelization AssignOp")
            }
        },
        Expr::And(var1, var2)
        | Expr::Or(var1, var2) => {
            check_expression_for_parallelizability(var1, global_vars, result, not_allowed_assignments) && check_expression_for_parallelizability(var1, global_vars, result, not_allowed_assignments)
        },
        Expr::ITE(var1, var2, var3) => {
            check_expression_for_global_vars(var1, global_vars, not_allowed_assignments)
                && check_expression_for_parallelizability(var2, global_vars, result, not_allowed_assignments)
                && check_expression_for_parallelizability(var3, global_vars, result, not_allowed_assignments)
        },
        Expr::Inc { is_inc, is_post, x } => {
            if !valid_lhs(x) {
                return false;
            }
            if not_allowed_assignments.iter().any(|val| val.contains(x)) {
                return false;
            }
            match x {
                Expr::Var(var) => {
                    let gl_var = GlobalVar::Scalar(var.clone());
                    if !global_vars.contains(&gl_var) {return true}
                    add_operator_for_global(gl_var, result, ParallelOp::Plus)
                },
                Expr::Unop(ast::Unop::Column, expr) => check_expression_for_global_vars(expr, global_vars, not_allowed_assignments),
                Expr::Index(var, ind) => {
                    if !check_expression_for_global_vars(ind, global_vars, not_allowed_assignments) {return false;} // in case ind contains global variable the expression is not parallelizable
                    let gl_var = index_into_gl_var(var, ind, global_vars);
                    match gl_var {
                        None => true,
                        Some(val ) => {
                            add_operator_for_global(val, result, ParallelOp::Plus)
                        }
                    }
                },
                _ => panic!("Unreachable statement in check_parallelization Inc")
            }
        },
        Expr::Index(var, ind) => {
            check_expression_for_parallelizability(var, global_vars, result, not_allowed_assignments) && check_expression_for_global_vars(ind, global_vars, not_allowed_assignments)
        },
        Expr::Call(func, args) => {
            let func = define_builtin(func);
            match func {
                Either::Left(val) => {
                    for val in args.iter() {
                        if !check_expression_for_global_vars(val, global_vars, not_allowed_assignments) {return false}
                    }
                    // panic!("Failed because of custom user function");
                    false  //work on this one
                }
                Either::Right(func) => {
                    for val in args.iter() {
                        if !check_expression_for_global_vars(val, global_vars, not_allowed_assignments) {return false}
                    }
                    let result = check_default_function_parallelizability(&func, args, global_vars);
                    // if(!result) {panic!("Failed because of function: {:?}", func);}
                    result
                }
            }
        },


        //check this one
        Expr::Getline {..} | Expr::ReadStdin | Expr::Cond(_) => false
    }
}

fn check_default_function_parallelizability<I: Clone + Hash + Eq+Debug>(func: &Function, args: &[&Expr<I>], global_vars: &HashSet<GlobalVar<I>>) -> bool {
    match func {
        Function::Unop(_) | Function::Binop(_) | Function::FloatFunc(_) | Function::IntFunc(_) => true,
        Function::Setcol | Function::Length | Function::Contains => true,
        Function::Match | Function::SubstrIndex | Function::Substr => true,
        Function::ToInt | Function::HexToInt | Function::ToUpper | Function::ToLower | Function::Rand => true,

        Function::Srand | Function::ReseedRng | Function::System => false,


        //check these functions further
        Function::GenSub | Function::Delete | Function::Clear => false,
        Function::EscapeCSV | Function::EscapeTSV | Function::JoinCSV | Function::JoinTSV | Function::JoinCols => false,
        Function::IncMap | Function::Exit => false,

        Function::Split => {
            if let Some(id) = args.get(1) {
                match id {
                    Expr::Var(v) => if global_vars.contains(&GlobalVar::ArrayUnknown(ArrayUnknown {id:v.clone(), fully_redefined:false})) {return false}
                    _ => panic!("Split is used with incorrect argument type")
                }
            }
            if let Some(id) = args.get(3) {
                match id {
                    Expr::Var(v) => if global_vars.contains(&GlobalVar::ArrayUnknown(ArrayUnknown {id:v.clone(), fully_redefined:false})) {return false}
                    _ => panic!("Split is used with incorrect argument type")
                }
            }
            true
        }
        Function::Sub | Function::GSub => true,

            _ => false
    }
}

fn check_expression_under_assignment<'a, 'b, I: IsSprintf + Clone + Hash + Eq+Debug>(i: &GlobalVar<'b, I>, expr: &Expr<'a, 'b, I>, global_vars: &HashSet<GlobalVar<'b, I>>, result: &mut HashMap<GlobalVar<'b, I>, ParallelOp>, index_expr: Option<&Expr<'a, 'b, I>>, not_allowed_assignments: &mut Vec<&'a Expr<'a, 'b, I>>) -> (bool, Option<ParallelOp>)
where Function: TryFrom<I>
{
    match expr {
        Expr::ILit(..) | Expr::FLit(..) | Expr::PatLit(..) | Expr::StrLit(..) => {(true, Some(ParallelOp::LastAssigned))}
        Expr::Var(var) => {
            let gl_var = GlobalVar::Scalar(var.clone());
            if gl_var == *i {return (true, None);}
            if global_vars.contains(&gl_var) {return (false, None);}
            (true, Some(ParallelOp::LastAssigned))
        }
        Expr::Index(var, ind) => {
            if !check_expression_for_global_vars(ind, global_vars, not_allowed_assignments) {return (false, None);}
            let gl_var = index_into_gl_var(var, ind, global_vars);
            match gl_var {
                None => (true, Some(ParallelOp::LastAssigned)),
                Some(GlobalVar::ArrayUnknown(val)) => {
                    if let GlobalVar::ArrayUnknown(i_val) = i {
                        if val.id == i_val.id && index_expr.unwrap() == *ind {
                            return (true, None)
                        }
                    }
                    if global_vars.contains(&GlobalVar::ArrayUnknown(ArrayUnknown {id: val.id.clone(), fully_redefined: false})) {return (false, None)}
                    if global_vars.contains(&GlobalVar::ArrayUnknown(ArrayUnknown {id: val.id.clone(), fully_redefined: true})) {return (false, None)}
                    (true, None)
                }, //adjust later
                Some(var) => {
                    if var == *i {return (true, None);}
                    (false, None)
                }
            }
        },
        Expr::Unop(_, var) => {
            if check_expression_for_global_vars(expr, global_vars, not_allowed_assignments) {
                (true, Some(ParallelOp::LastAssigned))
            } else {(false, None)}
        },
        a @ Expr::Binop(_, var1, var2)
        | a @ Expr::And(var1, var2)
        | a @ Expr::Or(var1, var2) => {
            let expr1 = check_expression_under_assignment(i, var1, global_vars, result, index_expr, not_allowed_assignments);
            if expr1.0 == false {return (false, None);}
            match expr1.1 {
                None => { //left part is variable i
                    let op = match find_par_operator(a) {
                        Some(op) => op,
                        None => return (false, None)
                    };
                    if check_expression_for_global_vars(var2, global_vars, not_allowed_assignments) {
                        (true, Some(op))
                    } else {(false, None)}
                }
                Some(ParallelOp::LastAssigned) => { //the left part is independent of a
                    let op = match find_com_par_operator(a) {
                        Some(op) => op,
                        None => return (false, None)
                    };
                    let expr2 = check_expression_under_assignment(i, var2, global_vars, result, index_expr, not_allowed_assignments);
                    if expr2.0 == false {return (false, None);}
                    match expr2.1 {
                        None => {(true, Some(op))}
                        Some(ParallelOp::LastAssigned) => {(true, Some(ParallelOp::LastAssigned))}
                        Some(op2) => {
                            if op == op2 {(true, Some(op))}
                            else {(false, None)}
                        }
                    }
                }
                Some(op1) => { //the left part is dependent on a with some operator
                    let op2 = match find_par_operator(a) {
                        Some(op) => op,
                        None => return (false, None)
                    };
                    if op1 != op2 {return (false, None)}
                    if check_expression_for_global_vars(var2, global_vars, not_allowed_assignments) {
                        (true, Some(op1))
                    } else {(false, None)}
                }
            }
        },
        a@ Expr::Assign(lhs, _) => check_nested_assignment(i, lhs, a, global_vars, result, index_expr, not_allowed_assignments),
        a @ Expr::AssignOp(lhs, _, _) => check_nested_assignment(i, lhs, a, global_vars, result, index_expr, not_allowed_assignments),
        a @ Expr::Inc{is_inc, is_post, x} => check_nested_assignment(i, x, a, global_vars, result, index_expr, not_allowed_assignments),
        a@ Expr::Call(_, _) => {
            if !check_expression_for_parallelizability(a, global_vars, result, not_allowed_assignments) {return (false, None)}
            (true, Some(ParallelOp::LastAssigned))
        },
        Expr::ITE {..} => {
            //TODO: check it in the future //
            (false, None)
        }
        Expr::Getline {..} | Expr::ReadStdin | Expr::Cond(_) => (false, None),
    }
}

fn check_nested_assignment<'a, 'b, I: IsSprintf + Clone + Hash + Eq+Debug>(i: &GlobalVar<'b, I>, lhs:&Expr<I>, expr: &Expr<'a, 'b, I>, global_vars: &HashSet<GlobalVar<'b, I>>, result: &mut HashMap<GlobalVar<'b, I>, ParallelOp>, index_expr: Option<&Expr<'a, 'b, I>>, not_allowed_assignments: &mut Vec<&'a Expr<'a, 'b, I>>) -> (bool, Option<ParallelOp>)
where Function: TryFrom<I>
{
    if !check_expression_for_parallelizability(expr, global_vars, result, not_allowed_assignments) {return (false, None)}
    match lhs {
        Expr::Var(var) => {
            let gl_var = GlobalVar::Scalar(var.clone());
            if gl_var == *i {return (true, None)}
            if global_vars.contains(&gl_var) {return (false, None)}
            (true, Some(ParallelOp::LastAssigned))
        },
        Expr::Unop(ast::Unop::Column, expr) => {
            if check_expression_for_global_vars(expr, global_vars, not_allowed_assignments) {
                (true, Some(ParallelOp::LastAssigned))
            } else {(false, None)}
        },
        Expr::Index(var, ind) => {
            if !check_expression_for_global_vars(ind, global_vars, not_allowed_assignments) {return (false, None);}
            let gl_var = index_into_gl_var(var, ind, global_vars);
            match gl_var {
                None => (true, Some(ParallelOp::LastAssigned)),
                Some(GlobalVar::ArrayUnknown(val)) => {
                    if let GlobalVar::ArrayUnknown(i_val) = i {
                        if val.id == i_val.id && index_expr.unwrap() == *ind {
                            return (true, None)
                        }
                    }
                    if global_vars.contains(&GlobalVar::ArrayUnknown(ArrayUnknown {id: val.id.clone(), fully_redefined: false})) {return (false, None)}
                    if global_vars.contains(&GlobalVar::ArrayUnknown(ArrayUnknown {id: val.id.clone(), fully_redefined: true})) {return (false, None)}
                    (true, None)
                },
                Some(var) => {
                    if var == *i {return (true, None);}
                    (false, None)
                }
            }
        },
        _ => panic!("Unreachable statement in check_parallelization Inc")
    }
}

fn add_operator_for_global<'a, I: Clone + Hash + Eq+Debug>(i: GlobalVar<'a, I>, results: &mut HashMap<GlobalVar<'a, I>, ParallelOp>, operator: ParallelOp) -> bool {
    let value = results.get(&i);
    match value{
        Some(op) => *op == operator,
        None => {
            results.insert(i, operator);
            true
        }
    }
}

//Function taken from cfg.rs
pub fn valid_lhs<I>(e: &Expr<I>) -> bool {
    use ast::Expr::*;
    matches!(e, Index(..) | Var(..) | Unop(ast::Unop::Column, _))
}


//Function to find parallel operator for the cases when commutative property does not matter
// (global variable is on the left side of the operator)
fn find_par_operator<I>(expr: &Expr<I>) -> Option<ParallelOp> {
    match expr {
        Expr::Binop(operator, _, _)
        | Expr::AssignOp(_, operator, _) => {
            match operator {
                Binop::Plus => Some(ParallelOp::Plus),
                Binop::Minus => Some(ParallelOp::Plus),
                Binop::Mult => Some(ParallelOp::Mult),
                Binop::Div => Some(ParallelOp::Mult),
                Binop::Concat => Some(ParallelOp::Concat),
                _ => None
            }
        }
        Expr::And(_, _) => Some(ParallelOp::And),
        Expr::Or(_, _) => Some(ParallelOp::Or),
        _ => None
    }
}


//Function to find parallel operator. Called in case commutative property should be hold (global variable is on the right side of operator).
// (e.g (a = a+1) == (a = 1 + a) but (a = a - 1) != (a = 1 - a)).
// In case of non-commutative operation, it can be parallelized only if global variable is on the left of the operator
fn find_com_par_operator<I>(expr: &Expr<I>) -> Option<ParallelOp> {
    match expr {
        Expr::Binop(operator, _, _)
        | Expr::AssignOp(_, operator, _) => {
            match operator {
                Binop::Plus => Some(ParallelOp::Plus),
                Binop::Mult => Some(ParallelOp::Mult),
                _ => None
            }
        }
        Expr::And(_, _) => Some(ParallelOp::And),
        Expr::Or(_, _) => Some(ParallelOp::Or),
        _ => None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Binop, Expr, Pattern, Stmt, Unop};
    use hashbrown::HashSet;
    use crate::arena::Arena;

    #[test]
    fn test_pattern_null() {
        let pattern: Pattern<'_, '_, &str> = Pattern::Null;
        let globals = HashSet::new();
        assert!(check_pattern_parallelizability(&pattern, &globals));
    }

    #[test]
    fn test_pattern_bool_no_globals() {
        let pattern = Pattern::Bool(&Expr::<&str>::ILit(1));
        let globals = HashSet::new();
        assert!(check_pattern_parallelizability(&pattern, &globals));
    }

    #[test]
    fn test_pattern_bool_with_global() {
        let pattern = Pattern::Bool(&Expr::Var("x"));
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        assert!(!check_pattern_parallelizability(&pattern, &globals));
    }

    #[test]
    fn test_pattern_bool_no_global_usage() {
        let pattern = Pattern::Bool(&Expr::Var("x"));
        let globals = HashSet::new();
        assert!(check_pattern_parallelizability(&pattern, &globals));
    }

    #[test]
    fn test_pattern_comma() {
        let pattern = Pattern::Comma(&Expr::<&str>::ILit(1), &Expr::ILit(2));
        let globals = HashSet::new();
        assert!(!check_pattern_parallelizability(&pattern, &globals));
    }

    #[test]
    fn test_expression_global_vars_literals() {
        let expr = Expr::<&str>::ILit(42);
        let globals = HashSet::new();
        assert!(check_expression_for_global_vars(&expr, &globals, &mut Vec::new()));
    }

    #[test]
    fn test_expression_global_vars_variable_no_global() {
        let expr = Expr::Var("x");
        let globals = HashSet::new();
        assert!(check_expression_for_global_vars(&expr, &globals, &mut Vec::new()));
    }

    #[test]
    fn test_expression_global_vars_variable_with_global() {
        let expr = Expr::Var("x");
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        assert!(!check_expression_for_global_vars(&expr, &globals, &mut Vec::new()));
    }

    #[test]
    fn test_expression_global_vars_binop_no_globals() {
        let expr = Expr::Binop(Binop::Plus, &Expr::Var("x"), &Expr::Var("y"));
        let globals = HashSet::new();
        assert!(check_expression_for_global_vars(&expr, &globals, &mut Vec::new()));
    }

    #[test]
    fn test_expression_global_vars_binop_with_global() {
        let expr = Expr::Binop(Binop::Plus, &Expr::Var("x"), &Expr::Var("y"));
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        assert!(!check_expression_for_global_vars(&expr, &globals, &mut Vec::new()));
    }

    #[test]
    fn test_expression_global_vars_assignment() {
        let expr = Expr::Assign(&Expr::Var("x"), &Expr::ILit(1));
        let globals = HashSet::new();
        assert!(check_expression_for_global_vars(&expr, &globals, &mut Vec::new()));
    }

    #[test]
    fn test_statement_expr_no_globals() {
        let stmt = Stmt::Expr(&Expr::<&str>::ILit(1));
        let globals = HashSet::new();
        let mut result = HashMap::new();
        assert!(check_statement_parallelizability(&stmt, &globals, &mut result, false));
    }

    #[test]
    fn test_statement_expr_with_global() {
        let stmt = Stmt::Expr(&Expr::Var("x"));
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        let mut result = HashMap::new();
        assert!(check_statement_parallelizability(&stmt, &globals, &mut result, false));
    }

    #[test]
    fn test_statement_block_empty() {
        let arena = Arena::default();
        let vec = Arena::new_vec_from_slice(&arena, &[]);
        let stmt: Stmt<'_, '_, &str> = Stmt::Block(vec);
        let globals = HashSet::new();
        let mut result = HashMap::new();
        assert!(check_statement_parallelizability(&stmt, &globals, &mut result, false));
    }

    #[test]
    fn test_statement_block_with_statements() {
        let arena = Arena::default();
        let vec = Arena::new_vec_from_slice(&arena, &[
            &Stmt::Expr(&Expr::<&str>::ILit(1)),
            &Stmt::Expr(&Expr::ILit(2))
        ]);
        let stmt = Stmt::Block(vec);
        let globals = HashSet::new();
        let mut result = HashMap::new();
        assert!(check_statement_parallelizability(&stmt, &globals, &mut result, false));
    }

    #[test]
    fn test_statement_if_no_else() {
        let stmt = Stmt::If(&Expr::<&str>::ILit(1), &Stmt::Expr(&Expr::ILit(2)), None);
        let globals = HashSet::new();
        let mut result = HashMap::new();
        assert!(check_statement_parallelizability(&stmt, &globals, &mut result, false));
    }

    #[test]
    fn test_statement_if_with_else() {
        let stmt = Stmt::If(&Expr::<&str>::ILit(1), &Stmt::Expr(&Expr::ILit(2)), Some(&Stmt::Expr(&Expr::ILit(3))));
        let globals = HashSet::new();
        let mut result = HashMap::new();
        assert!(check_statement_parallelizability(&stmt, &globals, &mut result, false));
    }

    #[test]
    fn test_statement_if_with_global_in_condition() {
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        let stmt = Stmt::If(&Expr::Var("x"), &Stmt::Expr(&Expr::ILit(1)), None);
        let mut result = HashMap::new();
        assert!(!check_statement_parallelizability(&stmt, &globals, &mut result, false));
    }

    #[test]
    fn test_statement_print_no_globals() {
        let stmt = Stmt::Print(&[&Expr::<&str>::ILit(1), &Expr::ILit(2)], None);
        let globals = HashSet::new();
        let mut result = HashMap::new();
        assert!(check_statement_parallelizability(&stmt, &globals, &mut result, false));
    }

    #[test]
    fn test_statement_print_with_global() {
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        let stmt = Stmt::Print(&[&Expr::Var("x")], None);
        let mut result = HashMap::new();
        assert!(!check_statement_parallelizability(&stmt, &globals, &mut result, false));
    }

    #[test]
    fn test_statement_print_strict_output_order() {
        // With strict_output_order=true, any print statement should return false
        let stmt = Stmt::Print(&[&Expr::<&str>::ILit(1)], None);
        let globals = HashSet::new();
        let mut result = HashMap::new();
        assert!(!check_statement_parallelizability(&stmt, &globals, &mut result, true));
    }

    #[test]
    fn test_statement_printf_strict_output_order() {
        // With strict_output_order=true, any printf statement should return false
        let stmt = Stmt::Printf(&Expr::<&str>::StrLit(b"hello"), &[], None);
        let globals = HashSet::new();
        let mut result = HashMap::new();
        assert!(!check_statement_parallelizability(&stmt, &globals, &mut result, true));
    }

    #[test]
    fn test_expression_parallelizability_literal() {
        let expr: ast::Expr<'_, '_, &str> = Expr::ILit(42);
        let globals = HashSet::new();
        let mut result = HashMap::new();
        assert!(check_expression_for_parallelizability(&expr, &globals, &mut result, &mut Vec::new()));
    }

    #[test]
    fn test_expression_parallelizability_assign_non_global() {
        let expr = Expr::Assign(&Expr::Var("x"), &Expr::ILit(1));
        let globals = HashSet::new();
        let mut result = HashMap::new();
        assert!(check_expression_for_parallelizability(&expr, &globals, &mut result, &mut Vec::new()));
        assert!(result.is_empty());
    }

    #[test]
    fn test_expression_parallelizability_assign_global_last_assigned() {
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        let expr = Expr::Assign(&Expr::Var("x"), &Expr::ILit(1));
        let mut result = HashMap::new();
        assert!(check_expression_for_parallelizability(&expr, &globals, &mut result, &mut Vec::new()));
        assert_eq!(result.get(&GlobalVar::Scalar("x")), Some(&ParallelOp::LastAssigned));
    }

    #[test]
    fn test_expression_parallelizability_assign_global_plus() {
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        let expr = Expr::Assign(&Expr::Var("x"), &Expr::Binop(Binop::Plus, &Expr::Var("x"), &Expr::ILit(1)));
        let mut result = HashMap::new();
        assert!(check_expression_for_parallelizability(&expr, &globals, &mut result, &mut Vec::new()));
        assert_eq!(result.get(&GlobalVar::Scalar("x")), Some(&ParallelOp::Plus));
    }

    #[test]
    fn test_expression_parallelizability_assign_op_plus() {
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        let expr = Expr::AssignOp(&Expr::Var("x"), Binop::Plus, &Expr::ILit(1));
        let mut result = HashMap::new();
        assert!(check_expression_for_parallelizability(&expr, &globals, &mut result, &mut Vec::new()));
        assert_eq!(result.get(&GlobalVar::Scalar("x")), Some(&ParallelOp::Plus));
    }

    #[test]
    fn test_expression_parallelizability_inc() {
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        let expr = Expr::Inc { is_inc: true, is_post: true, x: &Expr::Var("x") };
        let mut result = HashMap::new();
        assert!(check_expression_for_parallelizability(&expr, &globals, &mut result, &mut Vec::new()));
        assert_eq!(result.get(&GlobalVar::Scalar("x")), Some(&ParallelOp::Plus));
    }

    #[test]
    fn test_expression_parallelizability_assign_global_with_global_in_rhs() {
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        globals.insert(GlobalVar::Scalar("y"));
        let expr = Expr::Assign(&Expr::Var("x"), &Expr::Var("y"));
        let mut result = HashMap::new();
        assert!(!check_expression_for_parallelizability(&expr, &globals, &mut result, &mut Vec::new()));
    }

    #[test]
    fn test_multiple_assignments_in_expression() {
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        globals.insert(GlobalVar::Scalar("y"));
        // x = y = 1
        let inner_assign = Expr::Assign(&Expr::Var("y"), &Expr::ILit(1));
        let expr = Expr::Assign(&Expr::Var("x"), &inner_assign);
        let mut result = HashMap::new();
        // This should not be parallelizable because multiple globals are being assigned
        assert!(!check_expression_for_parallelizability(&expr, &globals, &mut result, &mut Vec::new()));
    }

    #[test]
    fn test_multiple_assignments_in_expression_safe() {
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        // x = y = 1
        let inner_assign = Expr::Assign(&Expr::Var("x"), &Expr::ILit(1));
        let expr = Expr::Assign(&Expr::Var("x"), &inner_assign);
        let mut result = HashMap::new();
        // This should not be parallelizable because multiple globals are being assigned
        assert!(check_expression_for_parallelizability(&expr, &globals, &mut result, &mut Vec::new()));
        assert_eq!(result.get(&GlobalVar::Scalar("x")), Some(&ParallelOp::LastAssigned));
    }

    #[test]
    fn test_not_parallelizable_multiple_operators() {
        let mut globals = HashSet::new();
        globals.insert(GlobalVar::Scalar("x"));
        // x = (y + x) * w  (mix of arithmetic and comparison operators)
        let plus_expr = Expr::Binop(Binop::Plus, &Expr::Var("y"), &Expr::Var("x"));
        let eq_expr = Expr::Binop(Binop::Mult, &plus_expr, &Expr::Var("w"));
        let expr = Expr::Assign(&Expr::Var("x"), &eq_expr);
        let mut result = HashMap::new();
        assert!(!check_expression_for_parallelizability(&expr, &globals, &mut result, &mut Vec::new()));
    }
}