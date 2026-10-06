#!/usr/bin/perl
# Phase-8.4 helper: compute what a seekable BGZF (bgzip) reader must read to
# serve byte range [A, A+LEN) of a `.bgz`: the whole `.gzi` index plus the
# compressed BGZF block(s) covering the range. BGZF blocks are self-contained
# gzip members, so the block's uncompressed length is its gzip trailer ISIZE —
# no decompression is needed to build the map. Emits shell-eval `KEY=VALUE`:
#
#   COST=<bytes> INDEX=<index size> FIRST=<uncompressed offset of first block>
#   BLOCKS=<comma list of "compressed_offset:compressed_size">
#
# Each block in BLOCKS can be `dd`'d out and decoded alone with `gzip -dc`.
use strict;
use warnings;

my ($bgz, $gzi, $A, $LEN) = @ARGV;
die "usage: $0 FILE.bgz FILE.bgz.gzi A LEN\n" unless defined $LEN;

my $isz = -s $gzi;
my $fssz = -s $bgz;
open(my $h, "<:raw", $bgz) or die "open $bgz: $!";

my $hi = $A + $LEN;
my ($off, $uoff, $cost, $first) = (0, 0, $isz, -1);
my @cover;
while ($off + 18 <= $fssz) {
    seek($h, $off, 0) or die "seek: $!";
    read($h, my $hd, 18) == 18 or last;
    my ($b0, $b1) = unpack("CC", substr($hd, 0, 2));
    last unless $b0 == 0x1f && $b1 == 0x8b;
    my $bsize = unpack("v", substr($hd, 16, 2)) + 1;
    seek($h, $off + $bsize - 4, 0) or die "seek trailer: $!";
    read($h, my $tl, 4) == 4 or last;
    my $isize = unpack("V", $tl);
    if ($uoff < $hi && $uoff + $isize > $A) {
        push @cover, "$off:$bsize";
        $cost += $bsize;
        $first = $uoff if $first < 0;
    }
    $uoff += $isize;
    $off += $bsize;
}
close($h);
print "COST=$cost INDEX=$isz FIRST=$first BLOCKS=" . join(",", @cover) . "\n";
