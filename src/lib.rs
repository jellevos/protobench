#![feature(generic_associated_types)]
#![feature(scoped_threads)]
#![doc = include_str!("../README.md")]
#![warn(missing_docs, unused_imports)]

use comm::{NetworkDescription, Channels};
use rayon::prelude::{IntoParallelRefMutIterator, IndexedParallelIterator, ParallelIterator};
use std::fmt::Debug;

use stats::{PartyStats, AggregatedStats};

/// Communication module, allows parties to send and receive messages.
pub mod comm;

/// Statistics module, allows parties to track timings and bandwidth costs.
pub mod stats;

/// A `Party` that takes part in a protocol. The party will receive a unique `id` when it is running the protocol, as well as
/// communication channels to and from all the other parties. A party keeps track of its own stats.
pub trait Party {
    type Input: Send;
    type Output: Debug + Send;

    fn get_name(&self, id: usize) -> String {
        format!("Party {}", id)
    }

    fn run(&mut self, id: usize, n_parties: usize, input: Self::Input, channels: Channels, stats: &mut PartyStats) -> Self::Output;
}

pub trait Protocol where Self: Debug {
    type Party: Party + Send;

    fn setup_parties(&self, n_parties: usize) -> Vec<Self::Party>;

    fn generate_inputs(&self, n_parties: usize) -> Vec<<Self::Party as Party>::Input>;

    fn validate_outputs(&self, outputs: &Vec<<Self::Party as Party>::Output>) -> bool {
        true
    }

    fn evaluate<N: NetworkDescription>(&self, n_parties: usize, network_description: &N, stats: &mut AggregatedStats, repetitions: usize) {
        let mut parties = self.setup_parties(n_parties);
        debug_assert_eq!(parties.len(), n_parties);

        for _ in 0..repetitions {
            let inputs = self.generate_inputs(n_parties);
            debug_assert_eq!(inputs.len(), n_parties);

            let channels = network_description.instantiate(n_parties);
            debug_assert_eq!(channels.len(), n_parties);

            let mut party_stats: Vec<PartyStats> = (0..n_parties).map(|_| PartyStats::new()).collect();

            let outputs = parties.par_iter_mut().enumerate().zip(inputs).zip(channels).zip(party_stats.par_iter_mut()).map(|((((id, party), input), channel), s)| party.run(id, n_parties, input, channel, s)).collect();

            if !self.validate_outputs(&outputs) {
                println!("The outputs are invalid:\n{:?} ...for these parameters:\n{:?}", outputs, self);
                // TODO: Mark invalid in stats
            }

            for s in party_stats {
                stats.incorporate_party_stats(s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use crate::{Party, PartyStats, Protocol, comm::{FullMesh, Channels}, stats::AggregatedStats};

    struct ExampleParty;

    impl Party for ExampleParty {
        type Input = usize;
        type Output = usize;

        fn run(&mut self, id: usize, n_parties: usize, input: Self::Input, mut channels: Channels, stats: &mut PartyStats) -> Self::Output {
            println!("Hi! I am {}/{}", id, n_parties - 1);

            let sending_timer = stats.create_timer("sending");
            for i in (id + 1)..n_parties {
                channels.send(&vec![id as u8], &i);
            }
            stats.stop_timer(sending_timer);

            for j in 0..id {
                println!(
                    "I am {}/{} and I received a message from {}",
                    id,
                    n_parties - 1,
                    channels.receive(&j).collect::<Vec<_>>()[0]
                );
            }

            id + input
        }
    }

    #[derive(Debug)]
    struct ExampleProtocol;

    impl Protocol for ExampleProtocol {
        type Party = ExampleParty;

        fn setup_parties(&self, n_parties: usize) -> Vec<Self::Party> {
            (0..n_parties).map(|_| ExampleParty).collect()
        }

        fn generate_inputs(&self, n_parties: usize) -> Vec<usize> {
            (0..n_parties).map(|_| 10).collect()
        }

        fn validate_outputs(&self, outputs: &Vec<<Self::Party as Party>::Output>) -> bool {
            for i in 0..outputs.len() {
                if outputs[i] != (10 + i) {
                    return false;
                }
            }

            true
        }
    }

    #[test]
    fn it_works() {
        let example = ExampleProtocol;
        let network = FullMesh;
        let mut stats = AggregatedStats::new("Stats".to_string());
        example.evaluate(5, &network, &mut stats, 1);

        println!("stats: {:?}", stats);
    }

    #[test]
    fn takes_longer() {
        let example = ExampleProtocol;

        let start = Instant::now();
        let network = FullMesh;
        let mut stats = AggregatedStats::new("Stats".to_string());
        example.evaluate(5, &network, &mut stats, 1);
        let duration_1 = start.elapsed();

        // FIXME: Add communication overhead
        // let start = Instant::now();
        // let (_, _) = example.evaluate_with_communication_overhead(
        //     5,
        //     vec![10; 5],
        //     vec![(); 5],
        //     Duration::from_secs(1),
        //     1.,
        // );
        // let duration_2 = start.elapsed();

        // assert!(duration_2 > duration_1);
        // assert!(duration_2 > Duration::from_secs(12));
    }
}
