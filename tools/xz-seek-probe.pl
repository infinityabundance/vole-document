#!/usr/bin/perl
# Phase-8.4 helper: compute what a seekable `.xz` reader must read to serve the
# byte range [A, A+LEN) of a blocked `.xz` (or a pixz `.pxz`, which is also a
# valid xz stream): the Stream Footer (12 B, read to find the Index), the Index
# field (which locates every block), and the padded compressed block(s) covering
# the range. This is the honest random-access cost of the format; it is NOT a
# sequential prefix read. Emits shell-eval `KEY=VALUE` pairs:
#
#   COST=<bytes> INDEX=<index size> FIRST=<uncompressed offset of first block>
#   BLOCKS=<comma list of 1-based block numbers>
#
# The BLOCKS list is passed to `tools/xz-block-reframe.pl`, which rebuilds a
# minimal single-stream `.xz` of exactly those blocks so they can be decoded in
# isolation (proving the blocks are independently decodable).
use strict;
use warnings;

my ($f, $A, $LEN) = @ARGV;
die "usage: $0 FILE.xz A LEN\n" unless defined $LEN;

my $sz = -s $f or die "cannot stat $f";
open(my $h, "<:raw", $f) or die "open $f: $!";
seek($h, $sz - 12, 0) or die "seek footer";
read($h, my $ft, 12) == 12 or die "short footer";
my $bw = unpack("V", substr($ft, 4, 4));
my $isz = 4 * ($bw + 1);
my $ioff = $sz - 12 - $isz;
seek($h, $ioff, 0) or die "seek index";
read($h, my $idx, $isz) == $isz or die "short index";
close($h);
die "bad index indicator" unless substr($idx, 0, 1) eq "\x00";

sub vli {
    my ($b, $p) = @_;
    my $v = 0;
    my $s = 0;
    while (1) {
        my $x = ord(substr($$b, $$p, 1));
        $$p++;
        $v |= ($x & 0x7F) << $s;
        last unless $x & 0x80;
        $s += 7;
    }
    return $v;
}

my $p = 1;
my $cnt = vli(\$idx, \$p);
my (@unp, @unc);
for (1 .. $cnt) {
    push @unp, vli(\$idx, \$p);
    push @unc, vli(\$idx, \$p);
}

my $hi = $A + $LEN;
my ($uoff, $cost, $first) = (0, $isz + 12, -1);
my @cover;
for my $i (1 .. $cnt) {
    my $padded = $unp[$i - 1] + ((4 - ($unp[$i - 1] % 4)) % 4);
    if ($uoff < $hi && $uoff + $unc[$i - 1] > $A) {
        push @cover, $i;
        $cost += $padded;
        $first = $uoff if $first < 0;
    }
    $uoff += $unc[$i - 1];
}
print "COST=$cost INDEX=$isz FIRST=$first BLOCKS=" . join(",", @cover) . "\n";
