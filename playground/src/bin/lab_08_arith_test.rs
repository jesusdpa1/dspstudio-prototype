use dsp_io::processing_graph::{ArithOpSpec};

fn main() {
    let ops = vec![
        ArithOpSpec::Add,
        ArithOpSpec::Subtract,
        ArithOpSpec::Multiply,
        ArithOpSpec::Divide,
    ];

    for op in ops {
        let json = serde_json::to_string(&op).unwrap();
        let decoded: ArithOpSpec = serde_json::from_str(&json).unwrap();
        println!("{:?} -> {} -> {:?}", op, json, decoded);
        assert_eq!(op, decoded);
    }
    println!("Serialization test passed!");
}
