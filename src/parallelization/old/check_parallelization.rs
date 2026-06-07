use std::collections::{HashMap, HashSet};
use crate::ast::Binop;
use crate::builtins;
use crate::cfg::FunctionName::MainLoop;
use crate::cfg::{Function, ProgramContext, PrimStmt, PrimExpr, PrimVal, Ident, Transition};
use crate::common::NumTy;
use crate::parallelization::check_parallelization::ParallelOp::LastAssigned;

pub fn check_parallelization<'a, I: std::hash::Hash + Eq>
(global_vars: &HashSet<I>, vars_dependent_on_global: &HashSet<Ident>, program_context: &ProgramContext<'a, I>)
    -> (bool, HashMap<NumTy, ParallelOp>) {
    let functions = &program_context.funcs;

    //connect with propagation code (duplicates for now)
    let shared = &program_context.shared;
    let mut global_var_ids = HashSet::new();
    // let mut id_to_global_map = HashMap::new();
    for var in global_vars {
        let id = shared.hm.get(var).unwrap().low;
        global_var_ids.insert(id);
    //     id_to_global_map.insert(id, var.clone());
    }

    for func in functions {
        if func.name == MainLoop{
            let check_transitions = check_transitions(&global_var_ids, &vars_dependent_on_global, &func);
            if !check_transitions {return (false, HashMap::new())}

            let check_parallelizability = check_parallelization_for_main(&global_var_ids, &func);
            eprintln!("Parallelizability: {:?}", check_parallelizability);
            return  check_parallelizability;
        }
    }
    (true, HashMap::new())
}
fn check_transitions<I>(global_var_ids: &HashSet<NumTy>, vars_dependent_on_global: &HashSet<Ident>, function: &Function<I>) -> bool{
    let cfg = &function.cfg;
    for edge in cfg.edge_references() {
        let transition = edge.weight();
        if let Transition(Some(PrimVal::Var(id))) = transition {
            if global_var_ids.contains(&id.low) || vars_dependent_on_global.contains(&id) {
                eprintln!("Transition based on global variable: {:?}", id);
                return false;
            };
        }
    }
    true
}

fn check_parallelization_for_main<I>
(global_var_ids: &HashSet<NumTy>, function: &Function<I>)
    -> (bool, HashMap<NumTy, ParallelOp>) {
    let cfg = &function.cfg;
    let mut results = HashMap::new();
    for node in cfg.node_indices() {
        let mut local_var_dependency = HashMap::new();
        for stmt in &cfg[node].q {
            match stmt {
                PrimStmt::AsgnVar(i, expr) => {
                    if global_var_ids.contains(&i.low) {
                        if !parseGlobalAssgn(&global_var_ids, i, expr, &mut results, &mut local_var_dependency) {return (false, results)}
                    }
                    else {
                        // parseLocalAssgn()
                        continue
                    }
                }
                PrimStmt::AsgnIndex(..) => panic!("Not implemented exception"),
                _ => continue
            }
        }
    }

    (true, HashMap::new())
}

fn parseGlobalAssgn(global_var_ids: &HashSet<NumTy>, i: &Ident, expr: &PrimExpr, results: &mut HashMap<NumTy, ParallelOp>, local_var_dependency: &mut HashMap<&Ident, (NumTy, ParallelOp)>) -> bool {
    match expr {
        PrimExpr::Val(PrimVal::Var(var)) => {
            if global_var_ids.contains(&var.low) {return i.low == var.low} //if global assigned to the same global do nothing and return, if to another global - fail

            if local_var_dependency.contains_key(var) {
                let local_dep = local_var_dependency.get(var).unwrap();

                // let current_res_value  = results.get(&i.low).copied().unwrap_or(local_dep.1);
                if i.low == local_dep.0 {
                    add_operator_for_global(&i, results, local_dep.1)
                } else {false}
            } else {
                add_operator_for_global(&i, results, LastAssigned)
            }
        },
        PrimExpr::CallBuiltin(op, vec) => {
            match op {
                builtins::Function::Binop(op @ (Binop::Plus | Binop::Mult)) => {
                    let firstVal = vec.get(0).unwrap();
                    let secondVal = vec.get(1).unwrap();
                    if let PrimVal::Var(var) = firstVal {
                        if global_var_ids.contains(&var.low) {
                            if var.low != i.low {return false}
                            //check case a = a + secondVal(should be local)
                            return if check_if_not_dependent(secondVal, &global_var_ids, local_var_dependency) {
                                add_operator_for_global(&i, results, ParallelOp::Plus)
                            } else { false }
                        }
                        if local_var_dependency.contains_key(var) {
                            let local_dep = local_var_dependency.get(var).unwrap();
                            if local_dep.0 != i.low || (local_dep.1 != LastAssigned && local_dep.1 == ParallelOp::Plus) {
                                return false;
                            }
                            //check case a = dep(a) + secondVal(should be local)
                            return if check_if_not_dependent(secondVal, &global_var_ids, local_var_dependency) {
                                add_operator_for_global(&i, results, ParallelOp::Plus)
                            } else { false }
                        }
                    }

                },

                // builtins::Function::Unop(v) => {checkUnary(i, &mut results);},
                // builtins::Function::FloatFunc(v) => {checkFloat(i, &mut results);},

                _ => { //for all other functions it can be only last assigned (if assigned independently of global variables)
                    for val in vec {
                        if let PrimVal::Var(op) = val {
                            if global_var_ids.contains(&op.low) || local_var_dependency.contains_key(op) {
                                return false;
                            }
                        }
                    }
                    return if results.get(&i.low).copied().unwrap_or(LastAssigned) != LastAssigned {
                        false
                    } else {
                        results.insert(i.low, LastAssigned);
                        true
                    }
                }


            }
            true
        },
        // PrimExpr::Phi(preds) => {
        //     let _ = preds.iter().map(|(_, id)| {
        //         if current_env.contains(&id.low) {
        //             current_env.insert(i.low);
        //             return
        //         }
        //     });
        //     return;
        // },
        // PrimExpr::Sprintf(_, args) => ,
        // PrimExpr::CallUDF(_, args) => ,
        // PrimExpr::Index(pv1, pv2) => ,
        // PrimExpr::IterBegin(pv) => ,
        // PrimExpr::HasNext(pv) => ,
        // PrimExpr::Next(pv) => ,
        // PrimExpr::LoadBuiltin(_) => ,
        _ => true,
    }
}

fn check_if_not_dependent(val: &PrimVal, global_var_ids: &HashSet<NumTy>, local_var_dependency: &mut HashMap<&Ident, (NumTy, ParallelOp)>) -> bool {
    if let PrimVal::Var(op) = val {
        if global_var_ids.contains(&op.low) || local_var_dependency.contains_key(op) {
            return false;
        };
        return true;
    };
    true
}

fn add_operator_for_global(i: &Ident, results: &mut HashMap<NumTy, ParallelOp>, operator: ParallelOp) -> bool {
    if results.get(&i.low).copied().unwrap_or(operator) != operator {
        false
    } else {
        results.insert(i.low, operator);
        true
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ParallelOp {
    Plus,
    Mult,
    And,
    Or,
    Concat,
    LastAssigned
}

