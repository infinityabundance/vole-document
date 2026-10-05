# Merge the two per-corpus baseline JSON files into one results object.
# Field-wise addition of the two `overall` summaries; rows and by_corpus are
# concatenated. All overall fields are numeric byte/file counts.
def plus($a; $b): reduce ($a | keys[]) as $k ({}; . + {($k): ($a[$k] + $b[$k])});

{
  units: .[0].units,
  delta_convention: .[0].delta_convention,
  rows: (.[0].rows + .[1].rows),
  summary: {
    overall: plus(.[0].summary.overall; .[1].summary.overall),
    by_corpus: (.[0].summary.by_corpus + .[1].summary.by_corpus)
  }
}
