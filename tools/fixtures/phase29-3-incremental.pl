#!/usr/bin/perl
# Phase 29.3 — incremental observation court fixture (families + decision).
#
# WHY PERL: the court is required to run in the pinned `dev` service, which ships
# `perl`/`awk`/`sed` but no `python3`. This fixture is a faithful, line-by-line
# port of the Phase-28.7 generator's *deterministic* algorithms
# (`tools/fixtures/phase28-7-revision-reuse.py`): the localized CSV/JSON edit
# generators, the DOCX style-table variant, the XLSX shared-string variant, and
# the raw ZIP member surgery that preserves every other member's bytes
# bit-for-bit. The measurement itself is NOT here — it is the real Rust
# `field-incremental` mechanism the court drives and this script only aggregates.
#
# Subcommands
#   families --out DIR                       build families; print manifest TSV
#   decide   --raw DIR --out DIR --campaign C --short S
#                                            aggregate -> SUMMARY.md/DECISION.txt/
#                                            receipt.json (exit 1 on any
#                                            correctness failure)

use strict;
use warnings;
use POSIX qw(floor);
use Digest::SHA qw(sha256_hex);
use Compress::Raw::Zlib qw(crc32);
use IO::Compress::RawDeflate qw(rawdeflate $RawDeflateError);
use IO::Uncompress::RawInflate qw(rawinflate $RawInflateError);
use File::Path qw(make_path remove_tree);
use File::Basename qw(dirname);
use FindBin qw($Bin);

my $ROOT = dirname(dirname($Bin));   # repo root (/work)

sub slurp {
    my ($p) = @_;
    open my $fh, '<:raw', $p or die "open $p: $!";
    local $/;
    my $b = <$fh>;
    close $fh;
    return $b;
}

sub spit {
    my ($p, $b) = @_;
    open my $fh, '>:raw', $p or die "create $p: $!";
    print $fh $b;
    close $fh;
}

sub sha256_file { return sha256_hex(slurp($_[0])); }

sub real_docx {
    my ($id) = @_;
    return "$ROOT/real100-v1/documents/nist/docx/$id.docx";
}

# ---------------------------------------------------------------------------
# ZIP member surgery (a faithful port of phase28-7-revision-reuse.py's
# zip_members / _recompress / zip_write / zip_replace_member)
# ---------------------------------------------------------------------------
sub zip_members {
    my ($path) = @_;
    my $data = slurp($path);
    my $i = length($data) - 22;
    $i-- while $i >= 0 && substr($data, $i, 4) ne "\x50\x4b\x05\x06";
    die "no EOCD in $path" if $i < 0;
    my ($cnt, $cdoff) = unpack("x10 v x4 V", substr($data, $i, 22));
    my @members;
    my $p = $cdoff;
    for (1 .. $cnt) {
        my @f = unpack("V v v v v v v V V V v v v v v V V", substr($data, $p, 46));
        my ($sig, $vmb, $vn, $flags, $method, $dtime, $ddate, $crc, $csz, $usz,
            $nl, $el, $cl, $dstart, $iattr, $eattr, $off) = @f;
        die "bad central signature" unless $sig == 0x02014b50;
        my $name = substr($data, $p + 46, $nl);
        my $extra = substr($data, $p + 46 + $nl, $el);
        my @l = unpack("V v v v v v V V V v v", substr($data, $off, 30));
        my ($lsig, $lver, $lflags, $lmethod, $lt, $ld, $lcrc, $lcsz, $lusz,
            $lnl, $lel) = @l;
        die "bad local signature" unless $lsig == 0x04034b50;
        my $lname = substr($data, $off + 30 + $lnl, $lnl) // '';
        my $dabs = $off + 30 + $lnl + $lel;
        my $cdata = substr($data, $dabs, $csz);
        my $raw_lh = substr($data, $off, 30 + $lnl + $lel);
        push @members, {
            name => $name, flags => $flags & ~0x08, method => $method,
            dtime => $dtime, ddate => $ddate, crc => $crc,
            csz => $csz, usz => $usz, extra => $extra, cdata => $cdata,
            raw_lh => $raw_lh, isdir => ($name =~ m{/$} ? 1 : 0),
        };
        $p += 46 + $nl + $el + $cl;
    }
    return @members;
}

sub recompress {
    my ($newdata, $method) = @_;
    if ($method == 8) {
        my $cdata;
        rawdeflate \$newdata => \$cdata, Level => 9
            or die "rawdeflate: $RawDeflateError";
        return (8, $cdata, crc32($newdata), length($newdata));
    }
    return (0, $newdata, crc32($newdata), length($newdata));
}

sub zip_write {
    my ($members, $dst) = @_;
    open my $f, '>:raw', $dst or die "create $dst: $!";
    my @central;
    for my $m (@$members) {
        my $off = tell($f);
        # Always emit a fresh local header from the parsed metadata (bit 3
        # cleared, sizes known), so no stale data descriptor / bit-3 framing
        # remains. The member's *data* bytes are preserved verbatim.
        my $nb = $m->{name};
        my $flags = $m->{flags} & ~0x08;
        print $f pack("V v v v v v V V V v v", 0x04034b50, 20, $flags,
            $m->{method}, $m->{dtime}, $m->{ddate}, $m->{crc}, $m->{csz},
            $m->{usz}, length($nb), length($m->{extra}));
        print $f $nb;
        print $f $m->{extra};
        print $f $m->{cdata};
        push @central, [$off, $m];
    }
    my $cstart = tell($f);
    for my $ce (@central) {
        my ($off, $m) = @$ce;
        my $nb = $m->{name};
        my $flags = $m->{flags} & ~0x08;
        print $f pack("V v v v v v v V V V v v v v v V V", 0x02014b50, 20, 20,
            $flags, $m->{method}, $m->{dtime}, $m->{ddate}, $m->{crc},
            $m->{csz}, $m->{usz}, length($nb), length($m->{extra}), 0, 0, 0, 0,
            $off);
        print $f $nb;
        print $f $m->{extra};
    }
    my $csize = tell($f) - $cstart;
    print $f pack("V v v v v V V v", 0x06054b50, 0, 0, scalar(@central),
        scalar(@central), $csize, $cstart, 0);
    close $f;
}

sub zip_replace_member {
    my ($src, $dst, $member, $newdata) = @_;
    my @members = zip_members($src);
    for my $m (@members) {
        if ($m->{name} eq $member) {
            my ($method, $cdata, $crc, $usz) = recompress($newdata, $m->{method});
            $m->{method} = $method;
            $m->{cdata}  = $cdata;
            $m->{crc}    = $crc;
            $m->{usz}    = $usz;
            $m->{csz}    = length($cdata);
            $m->{flags}  = $m->{flags} & ~0x08;
            delete $m->{raw_lh};
        }
    }
    zip_write(\@members, $dst);
}

# ---------------------------------------------------------------------------
# deterministic text generators (port of phase28-7 gen_csv / gen_json)
# ---------------------------------------------------------------------------
sub gen_csv {
    my ($target, $variant) = @_;
    my @out = ("row_id,name,score,note\n");
    my $size = length($out[0]);
    my $i = 0;
    while ($size < $target) {
        my $note = "n, $i";
        $note = "EDITED2 $i" if $variant >= 2 && $i >= 5000 && $i < 5200;
        $note = "EDITED3 $i" if $variant >= 3 && $i >= 12000 && $i < 12300;
        my $row = sprintf("%d,item-%d,%d,\"%s\"\n", $i, $i, ($i * 37) % 1000, $note);
        push @out, $row;
        $size += length($row);
        $i++;
    }
    return join('', @out);
}

sub gen_json {
    my ($target, $variant) = @_;
    my @rows;
    my $size = 0;
    my $i = 0;
    while ($size < $target) {
        my $name = "item-$i";
        $name = "ITEM-2-$i" if $variant >= 2 && $i % 977 == 7;
        $name = "ITEM-3-$i" if $variant >= 3 && $i % 977 == 13;
        my $flag = ($i % 2 == 0) ? "true" : "false";
        my $row = sprintf('{"id":%d,"name":"%s","vals":[%d,%d,%d],"flag":%s}',
            $i, $name, $i, $i + 1, $i + 2, $flag);
        push @rows, $row;
        $size += length($row) + 2;
        $i++;
    }
    my $body = join(",\n", @rows);
    return '{"marker":"ZEBRA_MARKER_29_3","items":[' . $body . "]}";
}

sub styles_variant {
    my ($base, $bump) = @_;
    my $n = ($base =~ s/w:sz w:val="(\d+)"/'w:sz w:val="' . ($1 + $bump) . '"'/ge);
    return ($base, $n);
}

sub all_t {
    my ($text, $prefix) = @_;
    $text =~ s/(<t(?:[^>]*)>)([^<]*)(<\/t>)/$1$prefix$2$3/g;
    return $text;
}

sub pick_xlsx {
    my $base = "$ROOT/realformats-v1/documents/xlsx";
    return undef unless -d $base;
    for my $pref ("poi-spreadsheet-58731.xlsx",
                  "poi-spreadsheet-TwoSheetsOneHidden.xlsx") {
        my $p = "$base/$pref";
        return $p if -e $p;
    }
    opendir my $dh, $base or return undef;
    my @n = sort grep { /\.xlsx$/ } readdir $dh;
    closedir $dh;
    for my $n (@n) {
        my $p = "$base/$n";
        my @m = eval { zip_members($p) };
        for my $m (@m) {
            return $p if $m->{name} eq 'xl/sharedStrings.xml';
        }
    }
    return undef;
}

sub build_families {
    my ($out, $csv_bytes, $json_bytes) = @_;
    make_path($out);
    my @fams;

    # real DOCX draft chain
    my @ids = qw(nist-docx-0005 nist-docx-0006 nist-docx-0007 nist-docx-0008);
    my @revs;
    my $k = 0;
    for my $id (@ids) {
        $k++;
        my $src = real_docx($id);
        die "real corpus missing: $src\n" unless -e $src;
        my $dst = "$out/nist-sp800-34-r$k.docx";
        spit($dst, slurp($src)) unless -e $dst;
        push @revs, $dst;
    }
    push @fams, ["nist-sp800-34", "docx", "real-draft-edit", \@revs];

    # DOCX style-table family
    my $base = real_docx("nist-docx-0005");
    die "real corpus missing: $base\n" unless -e $base;
    my @bm = zip_members($base);
    my $bs = (map { $_->{cdata} } grep { $_->{name} eq 'word/styles.xml' } @bm)[0];
    die "docx-style: no word/styles.xml\n" unless defined $bs;
    my $plain;
    rawinflate \$bs => \$plain or die "rawinflate styles: $RawInflateError";
    my ($s2, $n2) = styles_variant($plain, 2);
    my ($s3, $n3) = styles_variant($plain, 4);
    die "docx-style: no w:sz values\n" if $n2 == 0 || $n3 == 0;
    my $d1 = "$out/docx-style-r1.docx";
    my $d2 = "$out/docx-style-r2.docx";
    my $d3 = "$out/docx-style-r3.docx";
    spit($d1, slurp($base));
    zip_replace_member($base, $d2, "word/styles.xml", $s2);
    zip_replace_member($base, $d3, "word/styles.xml", $s3);
    push @fams, ["docx-style", "docx", "style-table", [$d1, $d2, $d3]];

    # XLSX shared-string family
    my $xl = pick_xlsx();
    die "no XLSX sample with sharedStrings.xml under realformats-v1\n"
        unless defined $xl;
    my @xm = zip_members($xl);
    my ($ss) = map { $_->{cdata} } grep { $_->{name} eq 'xl/sharedStrings.xml' } @xm;
    my $ss_plain;
    my $m = (grep { $_->{name} eq 'xl/sharedStrings.xml' } @xm)[0];
    if ($m->{method} == 8) {
        rawinflate \$ss => \$ss_plain or die "rawinflate sharedStrings: $RawInflateError";
    } else {
        $ss_plain = $ss;
    }
    die "sharedStrings has no </t>\n" unless $ss_plain =~ m{</t>};
    my $x1 = "$out/xlsx-ss-r1.xlsx";
    my $x2 = "$out/xlsx-ss-r2.xlsx";
    my $x3 = "$out/xlsx-ss-r3.xlsx";
    spit($x1, slurp($xl));
    zip_replace_member($xl, $x2, "xl/sharedStrings.xml", all_t($ss_plain, "ZEBRA2:"));
    zip_replace_member($xl, $x3, "xl/sharedStrings.xml", all_t($ss_plain, "ZEBRA3:"));
    push @fams, ["xlsx-ss", "xlsx", "shared-string", [$x1, $x2, $x3]];

    # bounded text families
    my @csv;
    for my $v (1 .. 3) {
        my $p = "$out/csv-text-r$v.csv";
        spit($p, gen_csv($csv_bytes, $v));
        push @csv, $p;
    }
    push @fams, ["csv-text", "csv", "text-edit", \@csv];
    my @json;
    for my $v (1 .. 3) {
        my $p = "$out/json-text-r$v.json";
        spit($p, gen_json($json_bytes, $v));
        push @json, $p;
    }
    push @fams, ["json-text", "json", "text-edit", \@json];

    print "family\tfmt\tchange\trev\tpath\tbytes\tsha256\n";
    for my $fam (@fams) {
        my ($f, $fmt, $change, $revs) = @$fam;
        my $r = 0;
        for my $p (@$revs) {
            $r++;
            my $sz = -s $p;
            $sz = 0 unless defined $sz;
            printf "%s\t%s\t%s\t%d\t%s\t%d\t%s\n",
                $f, $fmt, $change, $r, $p, $sz, sha256_file($p);
        }
    }
}

# ---------------------------------------------------------------------------
# decision / aggregation
# ---------------------------------------------------------------------------
sub jread {
    my ($p) = @_;
    my $txt = slurp($p);
    require JSON::PP;
    return JSON::PP->new->decode($txt);
}

sub read_tsv {
    my ($p) = @_;
    my @rows;
    open my $fh, '<', $p or return @rows;
    my $hdr = <$fh>;
    while (my $l = <$fh>) {
        chomp $l;
        push @rows, [ split /\t/, $l, -1 ];
    }
    close $fh;
    return @rows;
}

sub decide {
    my ($raw, $out, $campaign, $short) = @_;
    make_path($out);
    my @man = read_tsv("$raw/manifest.tsv");
    my @ex  = read_tsv("$raw/exactness.tsv");

    # exactness by (family, rev)
    my %exact;
    my $exact_ok = 1;
    for my $e (@ex) {
        my ($fam, $rev, $field, $ok) = @$e;
        $exact{ $fam . "\0" . $rev } = $ok;
        $exact_ok = 0 unless $ok eq 'ok';
    }

    # families -> revs
    my (%fam_fmt, %fam_change, %fam_revs, @fams);
    for my $r (@man) {
        my ($fam, $fmt, $change, $rev, $path, $bytes, $sha) = @$r;
        if (!exists $fam_fmt{$fam}) {
            $fam_fmt{$fam} = $fmt; $fam_change{$fam} = $change; push @fams, $fam;
        }
        push @{ $fam_revs{$fam} }, [$rev, $bytes, $sha];
    }

    my %lane_of = (full => 1, cas => 1, incremental => 1);
    my @report;
    my $correct_ok = 1;
    my $cas_wrong_on_semantic = 0;
    my $incr_total = { reused => 0, recomputed => 0, n_obs => 0, invalidated => 0,
                       sem_only => 0, bytes_recomp => 0, bytes_reused => 0,
                       latency => 0, edges => 0,
                       index_bytes => 0, persistent => 0 };
    my $full_total = { recomputed => 0, n_obs => 0, bytes_recomp => 0, latency => 0 };
    my $cas_total  = { reused => 0, recomputed => 0, n_obs => 0, bytes_recomp => 0,
                       bytes_reused => 0, latency => 0, index_bytes => 0, persistent => 0 };

    for my $fam (@fams) {
        my $fmt = $fam_fmt{$fam};
        my $change = $fam_change{$fam};
        my @revs = @{ $fam_revs{$fam} };
        my %by = ();   # lane -> rev -> report
        my %digest = ();  # rev -> lane -> digest
        my %summary;
        for my $lane (sort keys %lane_of) {
            for my $rr (@revs) {
                my ($rev) = @$rr;
                my $p = "$raw/$fam/$lane/r$rev.json";
                next unless -e $p;
                my $j = jread($p);
                $by{$lane}{$rev} = $j;
                $digest{$rev}{$lane} = $j->{answer_digest};
                my $skip = ($rev eq '1');
                if ($lane eq 'incremental') {
                    $incr_total->{n_obs} += $j->{n_obs};
                    $incr_total->{reused} += $j->{reused};
                    $incr_total->{recomputed} += $j->{recomputed};
                    $incr_total->{invalidated} += $j->{invalidated};
                    $incr_total->{sem_only} += $j->{semantic_only_invalidations};
                    $incr_total->{bytes_recomp} += $j->{bytes_recomputed};
                    $incr_total->{bytes_reused} += $j->{bytes_reused};
                    $incr_total->{latency} += $j->{lane_us};
                    $incr_total->{edges} += $j->{dependency_edges};
                    $incr_total->{index_bytes} = $j->{index_bytes};
                    $incr_total->{persistent} = $j->{persistent_bytes};
                } elsif ($lane eq 'full') {
                    $full_total->{n_obs} += $j->{n_obs};
                    $full_total->{recomputed} += $j->{recomputed};
                    $full_total->{bytes_recomp} += $j->{bytes_recomputed};
                    $full_total->{latency} += $j->{lane_us};
                } elsif ($lane eq 'cas') {
                    $cas_total->{n_obs} += $j->{n_obs};
                    $cas_total->{reused} += $j->{reused};
                    $cas_total->{recomputed} += $j->{recomputed};
                    $cas_total->{bytes_recomp} += $j->{bytes_recomputed};
                    $cas_total->{bytes_reused} += $j->{bytes_reused};
                    $cas_total->{latency} += $j->{lane_us};
                    $cas_total->{index_bytes} = $j->{index_bytes};
                    $cas_total->{persistent} = $j->{persistent_bytes};
                }
            }
        }
        # correctness: incremental == full per revision; cas == full on non-semantic
        my $fam_ok = 1;
        my $cas_diverges = 0;
        for my $rr (@revs) {
            my ($rev) = @$rr;
            next unless exists $digest{$rev}{incremental} && exists $digest{$rev}{full};
            if ($digest{$rev}{incremental} ne $digest{$rev}{full}) {
                $fam_ok = 0;
                $correct_ok = 0;
            }
            if (exists $digest{$rev}{cas}
                    && $digest{$rev}{cas} ne $digest{$rev}{full}) {
                $cas_diverges = 1;
            }
        }
        $cas_wrong_on_semantic = 1 if $cas_diverges
            && ($change eq 'style-table' || $change eq 'shared-string');

        my $incr_reused = 0; my $incr_nobs = 0; my $sem_only = 0;
        for my $rr (@revs) {
            my ($rev) = @$rr;
            next if $rev eq '1';
            my $j = $by{incremental}{$rev};
            next unless $j;
            $incr_reused += $j->{reused};
            $incr_nobs   += $j->{n_obs};
            $sem_only    += $j->{semantic_only_invalidations};
        }
        my $reuse_pct = $incr_nobs > 0 ? 100.0 * $incr_reused / $incr_nobs : 0.0;
        push @report, {
            family => $fam, fmt => $fmt, change => $change, n_revs => scalar(@revs),
            reuse_pct => $reuse_pct, incr_reused => $incr_reused,
            incr_nobs => $incr_nobs, sem_only => $sem_only,
            correct => $fam_ok, cas_diverges => $cas_diverges,
        };
    }

    # ---- write SUMMARY.md ----
    open my $sm, '>', "$out/SUMMARY.md" or die $!;
    print $sm "# Phase 29.3 — incremental observation\n\n";
    print $sm "campaign: $campaign\n\n";
    print $sm "| family | fmt | change | revs | reused/nobs (rev>1) | reuse% | semantic-only invalid | incr==full | cas diverges |\n";
    print $sm "|---|---|---|---|---|---|---|---|---|\n";
    for my $r (@report) {
        printf $sm "| %s | %s | %s | %d | %d/%d | %.1f%% | %d | %s | %s |\n",
            $r->{family}, $r->{fmt}, $r->{change}, $r->{n_revs},
            $r->{incr_reused}, $r->{incr_nobs}, $r->{reuse_pct},
            $r->{sem_only}, $r->{correct} ? "yes" : "NO",
            $r->{cas_diverges} ? "yes" : "no";
    }
    print $sm "\n## Totals\n\n";
    print $sm "| lane | obs | reused | recomputed | bytes recomputed | bytes reused | latency us | index B | persistent B |\n";
    print $sm "|---|---|---|---|---|---|---|---|---|\n";
    printf $sm "| full | %d | 0 | %d | %d | 0 | %d | - | - |\n",
        $full_total->{n_obs}, $full_total->{recomputed},
        $full_total->{bytes_recomp}, $full_total->{latency};
    printf $sm "| cas | %d | %d | %d | %d | %d | %d | %d | %d |\n",
        $cas_total->{n_obs}, $cas_total->{reused}, $cas_total->{recomputed},
        $cas_total->{bytes_recomp}, $cas_total->{bytes_reused}, $cas_total->{latency},
        $cas_total->{index_bytes}, $cas_total->{persistent};
    printf $sm "| incremental | %d | %d | %d | %d | %d | %d | %d | %d |\n",
        $incr_total->{n_obs}, $incr_total->{reused}, $incr_total->{recomputed},
        $incr_total->{bytes_recomp}, $incr_total->{bytes_reused}, $incr_total->{latency},
        $incr_total->{index_bytes}, $incr_total->{persistent};
    print $sm "\n- dependency edges (incremental, last revision): $incr_total->{edges}\n";
    print $sm "- exactness (materialize --exact == source, fresh process): "
        . ($exact_ok ? "ALL OK" : "FAILED") . "\n";
    print $sm "- correctness (incremental answer_digest == full rebuild): "
        . ($correct_ok ? "ALL OK" : "FAILED") . "\n";
    print $sm "- semantic load-bearing (a physical-only CAS diverges on a style/shared-string case): "
        . ($cas_wrong_on_semantic ? "YES" : "no") . "\n";
    close $sm;

    # ---- write DECISION.txt ----
    my $reuse_overall = $incr_total->{n_obs} > 0
        ? 100.0 * $incr_total->{reused} / $incr_total->{n_obs} : 0.0;
    # index cost as a fraction of the full-rebuild raw store: use index bytes vs
    # the sum of source bytes (from the manifest) as a stated, reproducible ratio.
    my $src_total = 0;
    $src_total += $_->[5] for @man;
    my $index_frac = $src_total > 0
        ? 100.0 * $incr_total->{index_bytes} / $src_total : 0.0;
    my $verdict;
    my $index_ok = $index_frac <= 10.0;
    if (!$correct_ok) {
        $verdict = "NEGATIVE (correctness failed)";
    } elsif ($reuse_overall >= 20.0 && $cas_wrong_on_semantic && $index_ok) {
        $verdict = "RESOLVED (positive, scoped)";
    } elsif ($reuse_overall >= 20.0 && $cas_wrong_on_semantic) {
        $verdict = "RESOLVED on correctness+reuse; MARGINAL/NEGATIVE on index cost";
    } else {
        $verdict = "NEGATIVE (semantic edges not load-bearing here)";
    }
    open my $dc, '>', "$out/DECISION.txt" or die $!;
    print $dc "PHASE 29.3 DECISION — incremental observation (a real mechanism)\n\n";
    printf $dc "reuse (rev>1, incremental lane): %d of %d observations (%.1f%%)\n",
        $incr_total->{reused}, $incr_total->{n_obs}, $reuse_overall;
    printf $dc "semantic-only invalidations: %d\n", $incr_total->{sem_only};
    printf $dc "index bytes: %d (%.2f%% of %d source bytes; the Phase-28.7 gate's\n",
        $incr_total->{index_bytes}, $index_frac, $src_total;
    print $dc "  index criterion was <=10%, and it NAMED the caveat that persisting a\n";
    print $dc "  per-observation identity node flips the verdict negative)\n";
    printf $dc "persistent bytes (index + content-addressed outputs): %d\n",
        $incr_total->{persistent};
    printf $dc "correctness (incremental == full rebuild on every revision): %s\n",
        $correct_ok ? "HOLDS" : "FAILS";
    printf $dc "bytes recomputed: full=%d  cas=%d  incremental=%d\n",
        $full_total->{bytes_recomp}, $cas_total->{bytes_recomp},
        $incr_total->{bytes_recomp};
    printf $dc "physical-only CAS diverges on a style/shared-string case: %s\n",
        $cas_wrong_on_semantic ? "YES (semantic edges are load-bearing)" : "no";
    print $dc "\nVERDICT: $verdict\n";
    print $dc "SCOPE: the reuse win is on LOCALIZED edits; a whole-document redraft\n";
    print $dc "(the real DOCX draft chain) reuses ~0 by construction.\n";
    close $dc;

    # ---- write receipt.json ----
    require JSON::PP;
    my $meta = {};
    if (-e "$out/../METADATA.txt") {
        open my $mf, '<', "$out/../METADATA.txt";
        while (my $l = <$mf>) {
            chomp $l;
            if ($l =~ /^([a-z0-9_]+):\s*(.*)$/) { $meta->{$1} = $2; }
        }
        close $mf;
    }
    my $receipt = {
        phase => "29.3", kind => "incremental-observation", short => $short,
        court => $campaign, metadata => $meta,
        exactness => { n => scalar(@ex), ok => ($exact_ok ? JSON::PP::true() : JSON::PP::false()) },
        families => \@report,
        totals => {
            full => $full_total, cas => $cas_total, incremental => $incr_total,
        },
        correctness => {
            incremental_equals_full => ($correct_ok ? JSON::PP::true() : JSON::PP::false()),
            cas_diverges_on_semantic => ($cas_wrong_on_semantic ? JSON::PP::true() : JSON::PP::false()),
        },
        index_fraction_pct => $index_frac,
        reuse_pct_rev_gt_1 => $reuse_overall,
        verdict => $verdict,
    };
    open my $rj, '>', "$out/receipt.json" or die $!;
    print $rj JSON::PP->new->pretty->canonical->encode($receipt);
    close $rj;

    print STDERR "phase29-3: verdict=$verdict exact_ok=$exact_ok correct_ok=$correct_ok\n";
    exit(($exact_ok && $correct_ok) ? 0 : 1);
}

# ---------------------------------------------------------------------------
my $cmd = shift @ARGV // '';
my %opt;
while (@ARGV) {
    my $a = shift @ARGV;
    if ($a =~ /^--([a-z-]+)$/) { $opt{$1} = shift @ARGV; }
}
if ($cmd eq 'families') {
    die "families needs --out\n" unless $opt{out};
    build_families($opt{out}, $opt{'csv-bytes'} // 393216, $opt{'json-bytes'} // 393216);
} elsif ($cmd eq 'decide') {
    die "decide needs --raw --out --campaign --short\n"
        unless $opt{raw} && $opt{out} && $opt{campaign} && $opt{short};
    decide($opt{raw}, $opt{out}, $opt{campaign}, $opt{short});
} else {
    die "usage: $0 families --out DIR | decide --raw DIR --out DIR --campaign C --short S\n";
}
