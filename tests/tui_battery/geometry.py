"""Reference geometry shared by terminal seeds and board comparisons."""

GEOMETRY = {"canvas": [1100, 700], "columns": 130, "rows": 31, "cell": [8.4, 22], "origin": [4, 9]}
COLUMNS, ROWS = GEOMETRY["columns"], GEOMETRY["rows"]
CELL_WIDTH, CELL_HEIGHT = GEOMETRY["cell"]
ORIGIN = tuple(GEOMETRY["origin"])
ALIGNMENT_EPSILON = 0.00001
