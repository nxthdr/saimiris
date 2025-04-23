@0xe4af8a0924a7def3;

struct Probe {
    dstAddr      @0 :Data;
    srcPort      @1 :UInt16;
    dstPort      @2 :UInt16;
    ttl          @3 :UInt8;
    protocol     @4 :Protocol;

    enum Protocol {
        tcp      @0;
        udp      @1;
        icmp     @2;
        icmpv6   @3;
    }
}
