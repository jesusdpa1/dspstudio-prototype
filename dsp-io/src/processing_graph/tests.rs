#[cfg(test)]
mod tests {
    use crate::processing_graph::spec::*;
    use crate::processing_graph::processor::*;
    use crate::processing_graph::ChannelId;
    use crate::processing_graph::ArithOpSpec;
    use std::collections::HashMap;

    #[test]
    fn test_topological_sort_simple() {
        let spec = ProcessingGraphSpec {
            nodes: vec![
                SpecNode::Channel { id: ChannelId::Physical(0) }, // 0
                SpecNode::Float { value: 2.0 },                   // 1
                SpecNode::Arithmetic { op: ArithOpSpec::Multiply }, // 2
                SpecNode::Output { source_id: ChannelId::Physical(0) }, // 3
            ],
            wires: vec![
                SpecWire { from_node: 0, from_output: 0, to_node: 2, to_input: 0 },
                SpecWire { from_node: 1, from_output: 0, to_node: 2, to_input: 1 },
                SpecWire { from_node: 2, from_output: 0, to_node: 3, to_input: 0 },
            ],
            sample_rate: 1000.0,
        };
        let processor = GraphProcessor::new(spec);
        let order = processor.topological_order().unwrap();
        
        let idx0 = order.iter().position(|&i| i == 0).unwrap();
        let idx1 = order.iter().position(|&i| i == 1).unwrap();
        let idx2 = order.iter().position(|&i| i == 2).unwrap();
        let idx3 = order.iter().position(|&i| i == 3).unwrap();
        
        assert!(idx0 < idx2);
        assert!(idx1 < idx2);
        assert!(idx2 < idx3);
    }

    #[test]
    fn test_evaluate_simple_arithmetic() {
        let spec = ProcessingGraphSpec {
            nodes: vec![
                SpecNode::Channel { id: ChannelId::Physical(0) },
                SpecNode::Float { value: 3.0 },
                SpecNode::Arithmetic { op: ArithOpSpec::Multiply },
            ],
            wires: vec![
                SpecWire { from_node: 0, from_output: 0, to_node: 2, to_input: 0 },
                SpecWire { from_node: 1, from_output: 0, to_node: 2, to_input: 1 },
            ],
            sample_rate: 1000.0,
        };
        let processor = GraphProcessor::new(spec);
        
        let mut raw = HashMap::new();
        raw.insert(ChannelId::Physical(0), vec![1.0, 2.0, 3.0]);
        
        let signals = processor.evaluate_graph(&raw, 3, &HashMap::new()).unwrap();
        
        let output = signals.get(&(2, 0)).unwrap().as_waveform().unwrap();
        assert_eq!(output, &[3.0, 6.0, 9.0]);
    }

    #[test]
    fn test_independent_groups() {
        let spec = ProcessingGraphSpec {
            nodes: vec![
                SpecNode::Channel { id: ChannelId::Physical(0) }, // 0
                SpecNode::Output { source_id: ChannelId::Physical(0) }, // 1
                SpecNode::Channel { id: ChannelId::Physical(1) }, // 2
                SpecNode::Output { source_id: ChannelId::Physical(1) }, // 3
            ],
            wires: vec![
                SpecWire { from_node: 0, from_output: 0, to_node: 1, to_input: 0 },
                SpecWire { from_node: 2, from_output: 0, to_node: 3, to_input: 0 },
            ],
            sample_rate: 1000.0,
        };
        let processor = GraphProcessor::new(spec);
        let mut groups = processor.independent_groups();
        groups.sort_by_key(|g| g.len());
        
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 2);
        assert_eq!(groups[1].len(), 2);
    }
}
