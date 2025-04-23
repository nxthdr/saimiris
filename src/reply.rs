use capnp::message::Builder;
use capnp::serialize;
use caracat::models::Reply;

use crate::probe::serialize_ip_addr;
use crate::reply_capnp::reply;

pub fn serialize_reply(agent_id: String, reply: &Reply) -> Vec<u8> {
    let mut message = Builder::new_default();
    {
        let mut r = message.init_root::<reply::Builder>();

        r.set_agent_id(&agent_id);
        r.set_time_received_ns(reply.capture_timestamp.as_nanos() as u64);

        // Reply fields
        r.set_reply_src_addr(&serialize_ip_addr(reply.reply_src_addr));
        r.set_reply_dst_addr(&serialize_ip_addr(reply.reply_dst_addr));
        r.set_reply_id(reply.reply_id);
        r.set_reply_size(reply.reply_size);
        r.set_reply_ttl(reply.reply_ttl);
        r.set_reply_quoted_ttl(reply.quoted_ttl);
        r.set_reply_protocol(reply.reply_protocol);
        r.set_reply_icmp_type(reply.reply_icmp_type);
        r.set_reply_icmp_code(reply.reply_icmp_code);

        // MPLS Labels
        let mpls_labels = &reply.reply_mpls_labels;
        let mut mpls_list_builder = r.reborrow().init_reply_mpls_label(mpls_labels.len() as u32);
        for (i, mpls_label) in mpls_labels.iter().enumerate() {
            let mut mpls_builder = mpls_list_builder.reborrow().get(i as u32);
            mpls_builder.set_label(mpls_label.label);
            mpls_builder.set_exp(mpls_label.experimental);
            mpls_builder.set_s_bit(mpls_label.bottom_of_stack);
            mpls_builder.set_ttl(mpls_label.ttl);
        }

        // Probe fields (from quoted packet)
        r.set_probe_src_addr(&serialize_ip_addr(reply.probe_src_addr));
        r.set_probe_dst_addr(&serialize_ip_addr(reply.probe_dst_addr));
        r.set_probe_id(reply.probe_id);
        r.set_probe_size(reply.probe_size);
        r.set_probe_ttl(reply.probe_ttl);
        r.set_probe_protocol(reply.probe_protocol);
        r.set_probe_src_port(reply.probe_src_port);
        r.set_probe_dst_port(reply.probe_dst_port);

        // RTT
        r.set_rtt(reply.rtt);
    }

    serialize::write_message_to_words(&message)
}
