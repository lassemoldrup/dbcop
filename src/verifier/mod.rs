use std::collections::{HashMap, HashSet};

use consistency::causal::Causal;
use consistency::ser::Chains;
use consistency::si::SIChains;
use db::history::Transaction;

pub fn transactional_history_verify(
    histories: &Vec<Vec<Transaction>>,
    id_vec: &Vec<(usize, usize, usize)>,
) {
    for session in histories.iter() {
        for transaction in session.iter() {
            if transaction.success {
                for event in transaction.events.iter() {
                    if !event.write && event.success {
                        let (i_node, i_transaction, i_event) = id_vec[event.value];
                        if event.value == 0 {
                            assert_eq!(i_node, 0);
                            assert_eq!(i_transaction, 0);
                            assert_eq!(i_event, 0);
                        } else {
                            let transaction2 = &histories[i_node - 1][i_transaction];
                            let event2 = &transaction2.events[i_event];
                            // println!("{:?}\n{:?}", event, event2);
                            if !transaction2.success {
                                println!("DIRTY READ");
                                return;
                            }
                            if !event2.write {
                                println!("READ FROM NON-WRITE");
                                return;
                            }
                            if event.variable != event2.variable {
                                println!("READ DIFFERENT VARIABLE");
                                return;
                            }
                            if event.value != event2.value {
                                println!("READ DIFFERENT VALUE");
                                return;
                            }
                        }
                    }
                }
            }
        }
    }

    // add code for serialization check

    let mut transaction_last_writes = HashMap::new();

    for (i_node, session) in histories.iter().enumerate() {
        for (i_transaction, transaction) in session.iter().enumerate() {
            if transaction.success {
                let mut last_writes = HashMap::new();
                for (i_event, event) in transaction.events.iter().enumerate() {
                    if event.write && event.success {
                        // goes first to last, so when finished, it is last write event
                        last_writes.insert(event.variable, i_event);
                    }
                }
                transaction_last_writes.insert((i_node + 1, i_transaction), last_writes);
            }
        }
    }

    // checking for non-committed read, non-repeatable read
    for (i_node, session) in histories.iter().enumerate() {
        for (i_transaction, transaction) in session.iter().enumerate() {
            let mut writes = HashMap::new();
            let mut reads: HashMap<usize, (usize, usize, usize)> = HashMap::new();
            if transaction.success {
                for (i_event, event) in transaction.events.iter().enumerate() {
                    if event.success {
                        if event.write {
                            writes.insert(event.variable, i_event);
                            reads.remove(&event.variable);
                        } else {
                            let (wr_i_node, wr_i_transaction, wr_i_event) = id_vec[event.value];
                            if let Some(pos) = writes.get(&event.variable) {
                                // checking if read the last write in same transaction
                                if !((i_node + 1 == wr_i_node)
                                    && (i_transaction == wr_i_transaction)
                                    && (*pos == wr_i_event))
                                {
                                    println!("LOST UPDATE");
                                    return;
                                }
                            // assert!(
                            //     (i_node + 1 == wr_i_node) && (i_transaction == wr_i_transaction) && (*pos == wr_i_event),
                            //     "update-lost!! event-{} of txn({},{}) read value from ({},{},{}) instead from the txn.",
                            //     i_event,
                            //     i_node + 1,
                            //     i_transaction,
                            //     wr_i_node,
                            //     wr_i_transaction,
                            //     wr_i_event
                            // );
                            } else {
                                if event.value != 0 {
                                    // checking if read the last write from other transaction
                                    if *transaction_last_writes
                                        .get(&(wr_i_node, wr_i_transaction))
                                        .unwrap()
                                        .get(&event.variable)
                                        .unwrap()
                                        != wr_i_event
                                    {
                                        println!("UNCOMMITTED READ");
                                        return;
                                    }
                                    // assert_eq!(
                                    //     *transaction_last_writes
                                    //         .get(&(wr_i_node, wr_i_transaction))
                                    //         .unwrap()
                                    //         .get(&event.variable)
                                    //         .unwrap(),
                                    //     wr_i_event,
                                    //     "non-committed read!! action-{} of txn({},{}) read value from ({},{},{}) instead from the txn.",
                                    //     i_event,
                                    //     i_node + 1,
                                    //     i_transaction,
                                    //     wr_i_node,
                                    //     wr_i_transaction,
                                    //     wr_i_event
                                    // );
                                }

                                if let Some((wr_i_node2, wr_i_transaction2, wr_i_event2)) =
                                    reads.get(&event.variable)
                                {
                                    // checking if the read the same write as the last read in same transaction
                                    if !((*wr_i_node2 == wr_i_node)
                                        && (*wr_i_transaction2 == wr_i_transaction)
                                        && (*wr_i_event2 == wr_i_event))
                                    {
                                        println!("NON REPEATABLE READ");
                                        return;
                                    }
                                    // assert!(
                                    //     (*wr_i_node2 == wr_i_node) && (*wr_i_transaction2 == wr_i_transaction) && (*wr_i_event2 == wr_i_event),
                                    //     "non-repeatable read!! action-{} of txn({},{}) read value from ({},{},{}) instead as the last read.",
                                    //     i_event,
                                    //     i_node + 1,
                                    //     i_transaction,
                                    //     wr_i_node,
                                    //     wr_i_transaction,
                                    //     wr_i_event,
                                    // )
                                }
                            }
                            reads.insert(event.variable, (wr_i_node, wr_i_transaction, wr_i_event));
                        }
                    }
                }
            }
        }
    }

    let n_sizes: Vec<_> = histories.iter().map(|ref v| v.len()).collect();
    let mut transaction_infos = HashMap::new();

    for (i_node, session) in histories.iter().enumerate() {
        for (i_transaction, transaction) in session.iter().enumerate() {
            let mut read_info = HashMap::new();
            let mut write_info = HashSet::new();
            if transaction.success {
                for event in transaction.events.iter() {
                    if event.success {
                        if event.write {
                            write_info.insert(event.variable);
                        } else {
                            let (wr_i_node, wr_i_transaction, wr_i_event) = id_vec[event.value];
                            if wr_i_node != i_node + 1 || wr_i_transaction != i_transaction {
                                if let Some((old_i_node, old_i_transaction)) =
                                    read_info.insert(event.variable, (wr_i_node, wr_i_transaction))
                                {
                                    // should be same, because repeatable read
                                    assert_eq!(old_i_node, wr_i_node);
                                    assert_eq!(old_i_transaction, wr_i_transaction);
                                }
                            }
                        }
                    }
                }
            }
            transaction_infos.insert((i_node + 1, i_transaction), (read_info, write_info));
        }
    }

    {
        {
            println!("Doing causal consistency check");
            let mut causal = Causal::new(&n_sizes, &transaction_infos);
            if causal.preprocess_vis() && causal.preprocess_co() {
                println!("History is causal consistent!");
                println!();
                println!("Doing serializable consistency check");
                let mut chains = Chains::new(&n_sizes, &transaction_infos);
                println!("{:?}", chains);
                if !chains.preprocess() {
                    println!("found cycle while processing wr and po order");
                }
                // println!("{:?}", chains);
                // println!("{:?}", chains.serializable_order_dfs());
                match chains.serializable_order_dfs() {
                    Some(order) => {
                        println!("Serializable progress of transactions");
                        for node_id in order {
                            print!("{} ", node_id);
                        }
                        println!();
                        println!("SER")
                    }
                    None => {
                        println!("No valid SER history");
                        println!();
                        {
                            println!("Doing snapshot isolation check");
                            let mut chains = SIChains::new(&n_sizes, &transaction_infos);
                            println!("{:?}", chains);
                            if !chains.preprocess() {
                                println!("found cycle while processing wr and po order");
                            }
                            // println!("{:?}", chains);
                            match chains.serializable_order_dfs() {
                                Some(order) => {
                                    let mut rw_map = HashMap::new();
                                    println!(
                                        "SI progress of transactions (broken in read and write)"
                                    );
                                    for node_id in order {
                                        let ent = rw_map.entry(node_id).or_insert(true);
                                        if *ent {
                                            print!("{}R ", node_id);
                                            *ent = false;
                                        } else {
                                            print!("{}W ", node_id);
                                            *ent = true;
                                        }
                                    }
                                    println!();
                                    println!("SI")
                                }
                                None => println!("No valid SI history\nNON-SI"),
                            }
                        }
                    }
                }
            } else {
                println!("no valid causal consistent history");
                println!("NON-CC");
            }
        }
    }
}
