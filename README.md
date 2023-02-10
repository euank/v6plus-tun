##  map-e tun

Rust code to setup a map-e ipv4-in-ipv6 tunnel.

This is meant to work with the NTT hikari setup you'll encounter in large parts of Japan.

### Modes of operation

There are two modes of operation at the moment:

1. map-e calculator - identical to http://ipv4.web.fc2.com/map-e.html

    This mode exists to validate that the map-e calculations match an existing known-to-work tool

2. ip tunnel creation - create the necessary linux interfaces and iptables rules to actually route ipv4 traffic

    This mode just shells out to linux utilities to accomplish everything. It's a glorified bash script, but whatever

### Future work

It's intended to eventually implement the full map-e and tunneling logic as a userspace daemon, but who knows if I'll ever get to that.

After all, linux tunnels work fine, why bother, right?

### License

MIT

Please note, the map-e calculator code is derived from javascript code with an
unclear license. The portions copied are pure math though, so probably not
copyrightable? Regardless, beware that copyright of that portion could be iffy.
