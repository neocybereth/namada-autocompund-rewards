use argmin::{
    core::{CostFunction, Executor},
    solver::neldermead::NelderMead,
};

fn calculate_compound_balance(
    principal: f64,
    apr: f64,
    fee: f64,
    frequency: f64,
    time_in_years: f64,
) -> f64 {
    let effective_rate = apr / frequency;
    let fee_per_interval = fee;

    let mut balance = principal;

    for _ in 0..(frequency * time_in_years) as usize {
        balance = balance * (1.0 + effective_rate) - fee_per_interval;
        if balance <= 0.0 {
            return 0.0;
        }
    }

    balance
}

struct CompoundingOptimization {
    principal: f64,
    apr: f64,
    fee: f64,
    time_in_years: f64,
}

impl CostFunction for CompoundingOptimization {
    type Param = f64;
    type Output = f64;

    fn cost(&self, frequency: &Self::Param) -> Result<Self::Output, argmin::core::Error> {
        if *frequency > 24.0 * 365.0 {
            return Ok(f64::MAX);
        }

        let balance = calculate_compound_balance(
            self.principal,
            self.apr,
            self.fee,
            *frequency,
            self.time_in_years,
        );

        if balance <= 0.0 {
            return Ok(f64::MAX);
        }

        Ok(-balance)
    }
}

#[derive(Clone, Debug)]
pub struct OptimizationResult {
    pub max_balance: f64,
    pub optimal_frequency: u64, // in hours
}

impl OptimizationResult {
    pub fn seconds_between_compunding(&self) -> f64 {
        365.0 * 24.0 * 60.0 * 60.0 / self.optimal_frequency as f64
    }

    pub fn hours_between_compounding(&self) -> f64 {
        self.seconds_between_compunding() / 60.0 / 60.0
    }

    pub fn hours_between_compounding_rounded(&self) -> f64 {
        self.round_up_to_next_multiple(self.hours_between_compounding(), 4.0)
    }

    pub fn days_between_compounding(&self) -> f64 {
        self.hours_between_compounding() / 24.0
    }

    pub fn days_between_compounding_rounded(&self) -> f64 {
        self.round_up_to_next_multiple(self.days_between_compounding(), 4.0)
    }

    fn round_up_to_next_multiple(&self, value: f64, n: f64) -> f64 {
        if n == 0.0 {
            panic!("n cannot be zero");
        }
        (value / n).ceil() * n
    }
}

pub fn compute_frequency_opt(principal: f64, apr: f64, fee: f64) -> Option<OptimizationResult> {
    let problem = CompoundingOptimization {
        principal,
        apr,
        fee,
        time_in_years: 1_f64,
    };

    let params = vec![1.0, 24.0 * 365.0 / 4.0];
    let solver = NelderMead::new(params);

    let result = Executor::new(problem, solver)
        .configure(|state| state.max_iters(1000))
        .run()
        .ok()?;

    let optimal_frequency = result.state().param.unwrap();
    let max_balance = -result.state().cost;

    Some(OptimizationResult {
        max_balance,
        optimal_frequency: optimal_frequency as u64,
    })
}

#[cfg(test)]
mod test {
    use super::{calculate_compound_balance, compute_frequency_opt};

    #[test]
    fn test() {
        let p = 3_000_000_f64;
        let apr = 0.118_f64;
        let res = compute_frequency_opt(p, apr, 5.0_f64).unwrap();

        assert!(res.max_balance - p >= p * apr);
        assert_eq!(res.hours_between_compounding(), 25.53935860058309);
    }

    #[test]
    fn test_1() {
        let p = 1000_f64;
        let apr = 0.09_f64;
        let res = compute_frequency_opt(p, apr, 0.005_f64).unwrap();

        assert!(res.max_balance - p >= p * apr - 0.06_f64);
        assert_eq!(res.hours_between_compounding(), 50.93023255813954);
    }

    #[test]
    pub fn test_2() {
        let res = calculate_compound_balance(1000.0, 0.05, 0.06, 81.0, 1.0);
        assert_eq!(res, 1046.272905533)
    }
}
