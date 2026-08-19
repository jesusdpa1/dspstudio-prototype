use dsp_io::processing_graph::{ProcessingGraphSpec, SpecNode, SpecWire, ArithOpSpec, ChannelId, GraphProcessor};
use std::collections::HashMap;

fn main() -> anyhow::Result<()> {
    let spec = ProcessingGraphSpec {
        nodes: vec![
            SpecNode::Float { value: 10.0 },
            SpecNode::Float { value: 2.0 },
            SpecNode::Arithmetic { op: ArithOpSpec::Add },      // 10 + 2 = 12
            SpecNode::Arithmetic { op: ArithOpSpec::Subtract }, // 10 - 2 = 8
            SpecNode::Arithmetic { op: ArithOpSpec::Multiply }, // 10 * 2 = 20
            SpecNode::Arithmetic { op: ArithOpSpec::Divide },   // 10 / 2 = 5
        ],
        wires: vec![
            SpecWire { from_node: 0, from_output: 0, to_node: 2, to_input: 0 },
            SpecWire { from_node: 1, from_output: 0, to_node: 2, to_input: 1 },
            SpecWire { from_node: 0, from_output: 0, to_node: 3, to_input: 0 },
            SpecWire { from_node: 1, from_output: 0, to_node: 3, to_input: 1 },
            SpecWire { from_node: 0, from_output: 0, to_node: 4, to_input: 0 },
            SpecWire { from_node: 1, from_output: 0, to_node: 4, to_input: 1 },
            SpecWire { from_node: 0, from_output: 0, to_node: 5, to_input: 0 },
            SpecWire { from_node: 1, from_output: 0, to_node: 5, to_input: 1 },
        ],
        sample_rate: 40000.0,
    };

    let processor = GraphProcessor::new(spec);
    let raw: HashMap<ChannelId, Vec<f32>> = HashMap::new();
    let compiled = processor.compile_filters()?;
    let sigs = processor.evaluate_graph(&raw, 1, &compiled)?;

    let add = sigs.get(&(2, 0)).unwrap().as_waveform().unwrap()[0];
    let sub = sigs.get(&(3, 0)).unwrap().as_waveform().unwrap()[0];
    let mul = sigs.get(&(4, 0)).unwrap().as_waveform().unwrap()[0];
    let div = sigs.get(&(5, 0)).unwrap().as_waveform().unwrap()[0];

    println!("Add: {}, Sub: {}, Mul: {}, Div: {}", add, sub, mul, div);

    assert_eq!(add, 12.0);
    assert_eq!(sub, 8.0);
    assert_eq!(mul, 20.0);
    assert_eq!(div, 5.0);

    println!("Full Arithmetic Logic test passed!");
    Ok(())
}
