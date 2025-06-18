# MTR-NG Technical Architecture

## Algorithm Overview

MTR-NG implements the exact same network probing algorithm as the original MTR tool, but written in modern Rust. The core algorithm sends ICMP echo requests with varying TTL values to trace the network path to a destination.

## Core Algorithm Flow

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   Send Batch    │───▶│  Collect Responses │───▶│ Update Statistics│
│                 │    │                  │    │                │
│ - One per hop   │    │ - Match sequences │    │ - Calculate RTT │
│ - Increment TTL │    │ - Extract timing  │    │ - Track loss    │
│ - Track sequence│    │ - Validate packets│    │ - Update averages│
└─────────────────┘    └──────────────────┘    └─────────────────┘
         │                        │                        │
         └────────────────────────┼────────────────────────┘
                                  ▼
                         ┌─────────────────┐
                         │    Restart?     │
                         │                 │
                         │ - Target reached│
                         │ - Too many ???  │
                         │ - Max hops      │
                         └─────────────────┘
```

## Data Structures

### SequenceEntry
```rust
struct SequenceEntry {
    index: usize,       // Which hop this packet is probing
    transit: bool,      // Is packet still in flight?
    saved_seq: u32,     // Host-specific sequence number
    send_time: Instant, // Exact time packet was sent
}
```

**Purpose**: Tracks individual packets in flight, enabling accurate RTT calculation.

### MtrSession
```rust
struct MtrSession {
    // Network configuration
    target_addr: IpAddr,
    packet_id: u16,
    
    // Algorithm state
    next_sequence: u16,              // Global sequence counter (33000-65535)
    sequence_table: HashMap<u16, SequenceEntry>, // Active packets
    batch_at: usize,                 // Current hop being probed
    num_hosts: usize,                // Number of discovered hops
    
    // Results
    hops: Vec<HopStats>,             // Per-hop statistics
}
```

## Sequence Management

### Sequence Number Space
- **Range**: 33000-65535 (like original MTR)
- **Wraparound**: Automatically cycles when reaching max
- **Uniqueness**: Each packet gets unique sequence number

### Packet Lifecycle
1. **Send**: Create `SequenceEntry`, store in `sequence_table`
2. **Response**: Lookup by sequence number, calculate RTT
3. **Complete**: Remove from table, update statistics

```rust
// Send packet
let seq = self.new_sequence(hop_index);
send_icmp_packet(target, ttl, packet_id, seq);

// Receive response  
if let Some((index, send_time)) = self.mark_sequence_complete(seq) {
    let rtt = receive_time.duration_since(send_time);
    self.hops[index].add_rtt(rtt);
}
```

## Batch Sending Algorithm

### Original MTR Behavior
The original MTR sends **one packet per hop per round**, cycling through all hops:

```
Round 1: Send TTL=1, TTL=2, TTL=3, ..., TTL=N
Round 2: Send TTL=1, TTL=2, TTL=3, ..., TTL=N  
Round 3: Send TTL=1, TTL=2, TTL=3, ..., TTL=N
```

### Our Implementation
```rust
async fn net_send_batch(&mut self, target: Ipv4Addr, send_socket: &Socket) -> Result<bool> {
    // Send one packet for current hop
    self.net_send_query(target, send_socket, self.batch_at).await?;
    
    // Check restart conditions
    let restart = self.batch_at >= max_hops || 
                  too_many_unknown ||
                  reached_target;
    
    if restart {
        self.batch_at = 0;  // Reset to hop 1
    } else {
        self.batch_at += 1; // Next hop
    }
    
    Ok(restart)
}
```

### Restart Conditions (from original MTR)
1. **Reached target**: Echo reply from destination
2. **Max hops**: Hit configured hop limit  
3. **Too many unknowns**: Consecutive timeouts (firewall)

## ICMP Packet Handling

### Packet Types Handled
1. **TimeExceeded**: Intermediate hop response
2. **EchoReply**: Final destination response
3. **DestinationUnreachable**: Network error

### Validation Process
```rust
fn extract_original_packet_info(payload: &[u8]) -> Option<(u16, u16)> {
    // Parse ICMP error payload
    // Extract original IP header
    // Find original ICMP header  
    // Return (packet_id, sequence)
}
```

**Critical**: Must validate that response corresponds to our sent packet by checking ID and sequence.

## RTT Calculation

### Previous Bug (FIXED)
```rust
// WRONG: Measuring from batch start to response
let batch_start = Instant::now();
send_all_packets();
collect_responses(); // RTT = now() - batch_start (WRONG!)
```

### Correct Implementation
```rust
// CORRECT: Store exact send time per packet
let send_time = Instant::now();
send_packet(seq);
sequence_table.insert(seq, SequenceEntry { send_time, ... });

// Later when response arrives
let rtt = receive_time.duration_since(send_time); // CORRECT!
```

## Socket Management

### Raw ICMP Sockets
```rust
let send_socket = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?;
let recv_socket = Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4))?;

// Non-blocking with short timeout
send_socket.set_nonblocking(true)?;
recv_socket.set_read_timeout(Some(Duration::from_millis(1)))?;
```

**Why separate sockets**: Allows simultaneous send/receive operations.

### TTL Manipulation
```rust
socket.set_ttl(hop_number)?;  // Set TTL before each send
```

**Critical**: TTL determines which hop will respond with TimeExceeded.

## Statistics Tracking

### Per-Hop Statistics (HopStats)
```rust
struct HopStats {
    hop: u8,           // Hop number (1-based)
    addr: Option<IpAddr>, // Responding IP address
    sent: u32,         // Packets sent
    received: u32,     // Responses received
    rtts: Vec<Duration>, // Individual RTT measurements
}
```

### Calculations
- **Loss %**: `(sent - received) / sent * 100`
- **Avg RTT**: Mean of all RTT measurements
- **Best/Worst**: Min/Max RTT values
- **StdDev**: Standard deviation of RTTs

## Performance Optimizations

### Fast Response Collection
```rust
// Try to collect multiple responses per iteration
for _ in 0..10 {
    match recv_socket.recv_from(&mut buffer) {
        Ok((size, _)) => process_response(size),
        Err(_) => break, // No more responses
    }
}
```

### Efficient Packet Parsing
- **Zero-copy where possible**: Use packet view structs
- **Early validation**: Reject invalid packets quickly
- **Minimal allocations**: Reuse buffers

### Non-blocking I/O
- **Send**: Never blocks on socket write
- **Receive**: Short timeout prevents hanging
- **Async**: Tokio for cooperative multitasking

## Error Handling

### Network Errors
- **Permission denied**: Suggest running with sudo
- **Network unreachable**: Handle gracefully
- **Socket errors**: Log and continue

### Packet Validation
- **Malformed packets**: Ignore and continue
- **Wrong sequence**: Log but don't error
- **Timeout**: Mark as packet loss

## Testing Strategy

### Unit Tests
```rust
#[test]
fn test_sequence_management() {
    let mut session = MtrSession::new(args).await?;
    let seq = session.new_sequence(0);
    assert!(session.sequence_table.contains_key(&seq));
}
```

### Integration Tests
- **RTT accuracy**: Compare with original mtr
- **Packet loss**: Test under various network conditions
- **Performance**: Measure throughput and latency

### Benchmarks
```rust
#[bench]
fn bench_packet_parsing(b: &mut Bencher) {
    b.iter(|| parse_icmp_packet(&test_data));
}
```

## Security Considerations

### Raw Socket Privileges
- **Requirement**: Must run as root or with CAP_NET_RAW
- **Risk**: Can craft arbitrary packets
- **Mitigation**: Validate all inputs, rate limiting

### Input Validation
- **Packet contents**: Validate all fields before processing
- **Target addresses**: Sanitize hostname inputs
- **Parameters**: Bounds check all numeric inputs

## Future Improvements

### IPv6 Support
```rust
// TODO: Implement IPv6 packet handling
match target_addr {
    IpAddr::V4(ipv4) => run_ipv4_trace(ipv4),
    IpAddr::V6(ipv6) => run_ipv6_trace(ipv6), // Not yet implemented
}
```

### Multi-threading
- **Send thread**: Dedicated sender for higher packet rates
- **Receive thread**: Dedicated receiver for better response handling
- **Statistics thread**: Background statistics calculation

### Advanced Features
- **MPLS tracking**: Parse MPLS labels in responses
- **AS number lookup**: BGP AS path information
- **Geolocation**: Map IPs to geographic locations

## Debugging Guide

### Key Debug Points
1. **Packet send**: Verify TTL and sequence assignment
2. **Response matching**: Check sequence number validation
3. **RTT calculation**: Ensure proper timing measurement
4. **Statistics**: Verify calculations match original mtr

### Debug Logging
```bash
RUST_LOG=debug sudo cargo run -- target 2>&1 | grep -E "(Sending|Got|RTT)"
```

### Common Issues
- **High RTT**: Usually sequence tracking problem
- **No responses**: Check ICMP parsing or privileges
- **Wrong hops**: Verify TTL setting and response validation

---

This architecture ensures our implementation maintains fidelity to the original MTR algorithm while leveraging Rust's safety and performance benefits. 