#!/usr/bin/perl
#
# extract.pl — ExifTool maker-note table extractor (code generator).
#
# This GENERATOR SCRIPT is original work, licensed BSD-2-Clause like the
# revelo core. Its OUTPUT, however, is a derivative of Image::ExifTool's
# tag tables (tag-id -> name mappings and PrintConv value enumerations),
# which are Copyright Phil Harvey, distributed under the same terms as
# Perl (Artistic License / GNU GPL). The emitted Rust therefore carries a
# GPL header and MUST live in the GPL-licensed `revelo-exiftool-tables`
# crate, never in a BSD crate.
#
# Usage:
#   perl extract.pl <exiftool_perl_lib_dir> <Vendor> <out_dir>
#
# e.g. perl extract.pl \
#   /opt/homebrew/Cellar/exiftool/13.55/libexec/lib/perl5 Apple src/generated
#
# It loads Image::ExifTool::<Vendor>, walks the %Main tag table, and emits
# <out_dir>/<vendor>.rs with `tag_name()` and `print_conv()` lookups for
# the entries we can translate losslessly (plain names + integer-keyed
# PrintConv hashes). Entries whose PrintConv is Perl code, a hash ref, or
# otherwise non-static are emitted name-only and counted in the summary.

use strict;
use warnings;

my ($libdir, $vendor, $outdir, $table_name) = @ARGV;
die "usage: extract.pl <perl_lib_dir> <Vendor> <out_dir> [TableName]\n"
    unless $libdir && $vendor && $outdir;
$table_name //= 'Main';

unshift @INC, $libdir;

my $mod = "Image::ExifTool::$vendor";
eval "require $mod";
die "failed to load $mod: $@\n" if $@;

no strict 'refs';
my $table = \%{"${mod}::${table_name}"};
die "no \%${mod}::${table_name} table found\n" unless %$table;

# Collect (tag_id, name, [ (val, str), ... ]) rows.
my @rows;
my ($n_tags, $n_printconv, $n_skipped_pc) = (0, 0, 0);

for my $key (sort { $a <=> $b } grep { /^\d+$/ } keys %$table) {
    my $tag_id = $key + 0;
    my $def = $table->{$key};

    # An arrayref means multiple conditional variants; take the first that
    # carries a Name (good enough for a name+enum table).
    if (ref $def eq 'ARRAY') {
        ($def) = grep { ref $_ eq 'HASH' && $_->{Name} } @$def;
        next unless $def;
    }

    my ($name, $pc);
    if (!ref $def) {
        $name = $def;            # shorthand: tag => 'Name'
    } elsif (ref $def eq 'HASH') {
        $name = $def->{Name};
        $pc   = $def->{PrintConv};
    } else {
        next;
    }
    next unless defined $name && length $name;

    my @pairs;
    if (defined $pc) {
        if (ref $pc eq 'HASH') {
            for my $pk (sort { $a <=> $b } grep { /^-?\d+$/ } keys %$pc) {
                my $pv = $pc->{$pk};
                next if ref $pv;            # nested ref -> skip
                push @pairs, [ $pk + 0, $pv ];
            }
            $n_printconv++ if @pairs;
            $n_skipped_pc++ if !@pairs;     # had a PrintConv we couldn't use
        } else {
            $n_skipped_pc++;                # code/expr PrintConv
        }
    }

    push @rows, [ $tag_id, $name, \@pairs ];
    $n_tags++;
}

sub rs_escape {
    my $s = shift;
    $s =~ s/\\/\\\\/g;
    $s =~ s/"/\\"/g;
    $s =~ s/\n/ /g;
    $s =~ s/\r//g;
    return $s;
}

my $lc = lc $vendor;
# Main table -> <vendor>.rs; a named sub-table (ProcessBinaryData, e.g.
# Canon CameraSettings) -> <vendor>_<table>.rs so it doesn't clobber Main.
my $suffix = ($table_name eq 'Main') ? '' : '_' . lc($table_name);
my $path = "$outdir/$lc$suffix.rs";
open my $fh, '>', $path or die "cannot write $path: $!\n";

print $fh <<"HEADER";
// GENERATED FILE — DO NOT EDIT BY HAND.
//
// Derived from Image::ExifTool::$vendor (Copyright Phil Harvey), which is
// distributed under the same terms as Perl: the Artistic License or the
// GNU General Public License. This file is therefore a derivative work
// under those terms and is distributed as part of the GPL-licensed
// `revelo-exiftool-tables` crate. See that crate's LICENSE and NOTICE.
//
// Regenerate with: crates/revelo-exiftool-tables/codegen/extract.pl

/// Maker-note tag id -> canonical ExifTool tag name.
pub fn tag_name(id: u32) -> Option<&'static str> {
    match id {
HEADER

for my $r (@rows) {
    my ($id, $name) = @$r;
    printf $fh "        0x%04x => Some(\"%s\"),\n", $id, rs_escape($name);
}

print $fh <<"MID";
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
MID

for my $r (@rows) {
    my ($id, undef, $pairs) = @$r;
    for my $p (@$pairs) {
        my ($val, $str) = @$p;
        printf $fh "        (0x%04x, %d) => Some(\"%s\"),\n", $id, $val, rs_escape($str);
    }
}

print $fh <<"FOOTER";
        _ => None,
    }
}
FOOTER

close $fh;

printf STDERR
    "%-10s -> %s : %d tags, %d with usable PrintConv (%d PrintConvs skipped as code/refs)\n",
    $vendor, $path, $n_tags, $n_printconv, $n_skipped_pc;
