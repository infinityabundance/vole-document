#!/usr/bin/perl
# Phase-8.4 seekable-baseline helper: build a minimal, valid single-stream `.xz`
# file that contains ONLY the requested blocks of a larger `.xz`, so the block(s)
# covering a byte range can be decoded *in isolation* — exactly what a seekable
# reader does. Used by `tools/seekable-baselines.sh`.
#
# Usage:  perl tools/xz-block-reframe.pl INPUT.xz OUT.xz BLOCK[,BLOCK...]
#
# The block geometry is read from the input's own xz Index field (located from
# the Stream Footer, so it is exactly what a random-access reader reads):
# compressed offsets follow from the index's per-block unpadded sizes (each block
# is padded to a multiple of 4), and the new stream is
#   stream header (12 B, copied verbatim)
#   + the raw block bytes (verbatim, including their padding)
#   + a fresh Index field (indicator + count + unpadded/uncompressed sizes +
#     zero padding + CRC32)
#   + a fresh Stream Footer (CRC32 over backward-size + flags, backward size,
#     flags, magic "YZ").
# If the tool's `--block-size` blocks were not independently decodable this would
# fail `xz -dc` and the caller would notice (the cmp would mismatch).
use strict;
use warnings;

sub crc32 {
    my ($d) = @_;
    my $c = 0xFFFFFFFF;
    for my $b (unpack("C*", $d)) {
        $c ^= $b;
        for (1 .. 8) {
            $c = ($c >> 1) ^ (0xEDB88320 & (-($c & 1)));
        }
        $c &= 0xFFFFFFFF;
    }
    return ($c ^ 0xFFFFFFFF) & 0xFFFFFFFF;
}

sub crc32_bytes { return pack("V", crc32($_[0])); }

sub vli_encode {
    my $n = shift;
    my $s = "";
    do {
        my $b = $n & 0x7F;
        $n >>= 7;
        $b |= 0x80 if $n;
        $s .= chr($b);
    } while ($n);
    return $s;
}

# Decode a VLI from $buf at position $$pos (advances it). Returns the value.
sub vli_decode {
    my ($buf, $pos) = @_;
    my $val = 0;
    my $shift = 0;
    while (1) {
        my $b = ord(substr($buf, $$pos, 1));
        $$pos++;
        $val |= ($b & 0x7F) << $shift;
        last unless $b & 0x80;
        $shift += 7;
    }
    return $val;
}

my ($in, $out, $spec) = @ARGV;
die "usage: $0 IN.xz OUT.xz BLOCK[,BLOCK...]\n" unless defined $spec;
my @want = split(/,/, $spec);

my $size = -s $in or die "cannot stat $in";
open(my $fh, "<:raw", $in) or die "open $in: $!";
read($fh, my $hdr, 12) == 12 or die "short header";

# --- Read the 12-byte Stream Footer and the Index field it points to ---------
seek($fh, $size - 12, 0) or die "seek footer";
read($fh, my $footer, 12) == 12 or die "short footer";
my $backward = unpack("V", substr($footer, 4, 4));   # fixed 4-byte LE
my $stream_flags = substr($footer, 8, 2);
my $index_size = 4 * ($backward + 1);
my $index_off = $size - 12 - $index_size;

seek($fh, $index_off, 0) or die "seek index";
read($fh, my $index, $index_size) == $index_size or die "short index";
die "bad index indicator" unless substr($index, 0, 1) eq "\x00";
my $pos = 1;
my $count = vli_decode($index, \$pos);
my @unpadded;
my @uncomp;
for (1 .. $count) {
    push @unpadded, vli_decode($index, \$pos);
    push @uncomp,   vli_decode($index, \$pos);
}

# --- Locate each requested block (compressed offset from cumulative padding) -
my @rec;
my $body = "";
my $off = 12;
for my $i (1 .. $count) {
    my $padded = $unpadded[$i - 1] + ((4 - ($unpadded[$i - 1] % 4)) % 4);
    if (grep { $_ == $i } @want) {
        seek($fh, $off, 0) or die "seek block $i: $!";
        read($fh, my $raw, $padded) == $padded or die "short read block $i";
        $body .= $raw;
        push @rec, [ $unpadded[$i - 1], $uncomp[$i - 1] ];
    }
    $off += $padded;
}
die "wanted a block outside 1..$count\n" unless @rec == @want;
close($fh);

# --- Re-emit the stream header, the chosen blocks, a fresh Index, a Footer ---
my $idx = "\x00" . vli_encode(scalar @rec);
$idx .= vli_encode($_->[0]) . vli_encode($_->[1]) for @rec;
$idx .= "\x00" x ((4 - (length($idx) % 4)) % 4);
my $index_field = $idx . crc32_bytes($idx);
# The Stream Footer "Backward Size" is a fixed 4-byte LE (index_size/4 - 1).
my $back = pack("V", length($index_field) / 4 - 1);
my $f = crc32_bytes($back . $stream_flags) . $back . $stream_flags . "\x59\x5A";

open(my $ofh, ">:raw", $out) or die "open $out: $!";
print $ofh $hdr, $body, $index_field, $f;
close($ofh);
