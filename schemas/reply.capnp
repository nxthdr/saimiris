@0xb9d4bebd3a8258c7;

struct Reply {
    timeReceivedNs      @0  :UInt64;
    agentId             @1  :Text;
    replySrcAddr        @2  :Data;
    replyDstAddr        @3  :Data;
    replyId             @4  :UInt16;
    replySize           @5  :UInt16;
    replyTtl            @6  :UInt8;
    replyQuotedTtl      @7 :UInt8;
    replyProtocol       @8  :UInt8;
    replyIcmpType       @9  :UInt8;
    replyIcmpCode       @10  :UInt8;
    replyMplsLabel      @11 :List(Mpls);
    probeSrcAddr        @12 :Data;
    probeDstAddr        @13 :Data;
    probeId             @14 :UInt16;
    probeSize           @15 :UInt16;
    probeTtl            @16 :UInt8;
    probeProtocol       @17 :UInt8;
    probeSrcPort        @18 :UInt16;
    probeDstPort        @19 :UInt16;
    rtt                 @20 :UInt16;
}

struct Mpls {
    label               @0  :UInt32;
    exp                 @1  :UInt8;
    sBit                @2  :Bool;
    ttl                 @3  :UInt8;
}
