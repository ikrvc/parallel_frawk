use std::collections::{HashMap, HashSet, VecDeque};
use crate::cfg::FunctionName::MainLoop;
use crate::cfg::{Function, ProgramContext, PrimStmt, PrimExpr, PrimVal, Ident};
use crate::common::NumTy;

pub fn check_global_variable_dependency<'a, I: std::hash::Hash + std::cmp::Eq>(global_vars: &HashSet<I>, program_context: &ProgramContext<'a, I>)
                                                                               -> (bool, HashSet<Ident>) {
    let shared = &program_context.shared;
    let mut global_var_ids = HashSet::new();
    for var in global_vars {
        global_var_ids.insert(shared.hm.get(var).unwrap().low);
    }
    let functions = &program_context.funcs;
    for func in functions {
        if func.name == MainLoop{
            let check  = find_global_dependent_variables(&global_var_ids, func);
            eprintln!("Variables dependent on globals: {:?}", check);
            return check;
        }
    }
    (true, HashSet::new())
}

fn find_global_dependent_variables<'a, I>(global_var_ids: &HashSet<NumTy>, main_function: &Function<I>)
                                          -> (bool, HashSet<Ident>) {
    let cfg = &main_function.cfg;
    let mut dependent = HashMap::new();
    let mut current_env = HashSet::new();
    // let mut dependencies  = HashMap::new();
    for node in cfg.node_indices() {
        dependent.insert(node, HashSet::new());
    }

    let mut queue = VecDeque::new();
    for node in cfg.node_indices(){
        queue.push_back(node);
    }

    while !queue.is_empty() {
        let node = queue.pop_front().unwrap();
        let dependent_vars = dependent.get(&node).unwrap();
        for stmt in &cfg[node].q {
            // if !parse_statement(&stmt, global_var_ids, &mut current_env, &mut dependencies){
            //     return (false, dependencies);
            // }
            parse_statement(&stmt, global_var_ids, &mut current_env);
        }
        if dependent_vars != &current_env {
            for succ in cfg.neighbors(node) {
                queue.push_back(succ);
            }
            dependent.insert(node, current_env.clone());
        }
    }
    eprintln!("Current env: {:?}", current_env);
    (true, current_env)
}

fn parse_statement(stmt: &PrimStmt, global_var_ids: &HashSet<NumTy>, current_env: &mut HashSet<Ident>
                   // , dependencies: &mut HashMap<Ident, NumTy>
) {
    match stmt {
        PrimStmt::AsgnVar(i, expr) => {
            if global_var_ids.contains(&i.low) {return;}
            match expr {
                PrimExpr::Val(PrimVal::Var(id_main)) => {
                    if current_env.contains(&id_main) || global_var_ids.contains(&id_main.low){
                        current_env.insert(*i);
                    }
                    // find_common_dependency(*i, *id_main, global_var_ids, dependencies)
                },
                PrimExpr::Val(_) => return,
                PrimExpr::CallBuiltin(_, vect) => {
                    for val in vect {
                        if let PrimVal::Var(v) = val {
                            if current_env.contains(&v) || global_var_ids.contains(&v.low) {
                                current_env.insert(*i);
                            };
                            // if !find_common_dependency(*i, *v, global_var_ids, dependencies) {return false;};
                        }
                    }
                    return
                },
                PrimExpr::Phi(preds) => {
                    for (_, id) in preds {
                        if current_env.contains(&id) || global_var_ids.contains(&id.low) {
                            current_env.insert(*i);
                        };
                        // if !find_common_dependency(*i, *id, global_var_ids, dependencies) {return false;};
                    }
                    return
                },
                // PrimExpr::Sprintf(_, args) => ,
                // PrimExpr::CallUDF(_, args) => ,
                // PrimExpr::Index(pv1, pv2) => ,
                // PrimExpr::IterBegin(pv) => ,
                // PrimExpr::HasNext(pv) => ,
                // PrimExpr::Next(pv) => ,
                // PrimExpr::LoadBuiltin(_) => ,
                _ => return
            }
        }
        PrimStmt::AsgnIndex(..) => panic!("Not implemented exception"),
        _ => return
    }
}

// fn find_common_dependency(dependent: Ident, dependency: Ident, global_var_ids: &HashSet<NumTy>, dependencies: &mut HashMap<Ident, NumTy>) -> bool {
//     eprintln!("Dependent: {:?}, dependency: {:?}", dependent, dependency);
//     if global_var_ids.contains(&dependent.low) && global_var_ids.contains(&dependency.low) {return false} //if one global depends on another
//     if global_var_ids.contains(&dependent.low) {
//         if dependencies.contains_key(&dependency){
//             // return dependent.low == *dependencies.get(&dependency).unwrap() // if global depends on local which depends on global (it should be the same global))
//             return false;
//         } else {return true}; //if global depends on local without any dependency on other global
//     }
//     if global_var_ids.contains(&dependency.low) {
//         if dependencies.contains_key(&dependent){
//             // return dependency.low == *dependencies.get(&dependent).unwrap() // if local depends twice on global, it should be the same global
//             return false;
//         } else {
//             dependencies.insert(dependent, dependency.low); // if local depends on a new global, insert it here
//             return true;
//         }
//     }
//
//     if dependencies.contains_key(&dependent) && dependencies.contains_key(&dependency) {
//         // return dependencies.get(&dependent).unwrap() == dependencies.get(&dependency).unwrap()
//         return false;
//     } // if both variables were dependent on some global
//     if dependencies.contains_key(&dependency) {
//         dependencies.insert(dependent, *dependencies.get(&dependency).unwrap()); // if variable depends on some other which depends on global
//     }
//     true
// }
