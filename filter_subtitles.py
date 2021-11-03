import ass
import sys

with open(sys.argv[1]) as f:
    doc = ass.parse(f)
    print("styles:")
    for i, s in enumerate(doc.styles):
        print(f"#{i} - {s.name}")
    styles_to_filter = input(
        "styles to filter out (comma-separated list or 'none' to include all)\n> "
    )
    if styles_to_filter.strip() == "" or styles_to_filter.strip() == "none":
        styles_to_filter = []
    else:
        styles_to_filter = [
            doc.styles[int(i)].name for i in styles_to_filter.split(",")
        ]

    layer_numbers = set()
    for ev in doc.events:
        layer_numbers.add(ev.layer)

    layer_numbers = sorted(list(layer_numbers))

    print("layers:")
    print(", ".join([str(n) for n in layer_numbers]))
    layers_to_filter = input(
        "layer to filter out (comma-separated list or 'none' to include all)\n> "
    )
    if layers_to_filter.strip() == "" or layers_to_filter.strip() == "none":
        layers_to_filter = []
    else:
        layers_to_filter = [int(i) for i in layers_to_filter.split(",")]

    doc.events._lines = list(
        filter(
            lambda l: l.style not in styles_to_filter
            and l.layer not in layers_to_filter,
            doc.events._lines,
        )
    )

    with open(sys.argv[2], "w", encoding='utf_8_sig') as outf:
        doc.dump_file(outf)
    # print(doc.styles)
