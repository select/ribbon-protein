"""Geometric regression tests for the pen-plot export. Run: uv run --with shapely python -m unittest -v"""
import tempfile
import unittest
from pathlib import Path
import xml.etree.ElementTree as ET

import shapely
from shapely.geometry import LineString, Polygon

from plotter import (Panel, SVG, build_parser, continuous_tracks, hatch_loops, merge_lines,
                     merge_directed_lines, order_paths, read_panels, reinforce_borders, visible_lines)


class PlotterTests(unittest.TestCase):
    def test_latest_style_is_default(self):
        args = build_parser().parse_args(['source.svg', '-o', 'plot.svg'])
        self.assertTrue(args.fill_loops)
        self.assertEqual(args.stroke_direction, 'backbone')
        self.assertEqual(args.border_passes, 3)
        self.assertEqual(args.loop_spacing, 0.15)
        self.assertEqual(args.border_spacing, 0.10)
        args = build_parser().parse_args(['source.svg', '-o', 'plot.svg',
                                          '--no-fill-loops', '--border-passes', '1',
                                          '--stroke-direction', 'travel'])
        self.assertEqual(args.stroke_direction, 'travel')
        self.assertFalse(args.fill_loops)
        self.assertEqual(args.border_passes, 1)

    def test_default_gallery_exports_are_only_pen_paths(self):
        for path in Path('output/gallery').glob('*.svg'):
            with self.subTest(path=path):
                root = ET.parse(path).getroot()
                self.assertEqual(root.attrib['width'], '210mm')
                self.assertEqual(root.attrib['height'], '297mm')
                self.assertTrue(root.findall('.//' + SVG + 'path'))
                for element in root.iter():
                    self.assertIn(element.tag.removeprefix(SVG), ('svg', 'title', 'desc', 'g', 'path'))
                    self.assertEqual(element.attrib.get('fill', 'none'), 'none')

    def test_hidden_spans_removed_but_front_ink_kept(self):
        rear = Panel(Polygon([(-2, -2), (2, -2), (2, 2), (-2, 2)]),
                     [LineString([(-2, 0), (2, 0)])])
        front = Panel(Polygon([(-1, -1), (1, -1), (1, 1), (-1, 1)]),
                      [LineString([(-1, 0.5), (1, 0.5)])])
        lines = visible_lines([rear, front])
        self.assertEqual(len(lines), 3)
        self.assertAlmostEqual(sum(line.length for line in lines), 4)
        self.assertTrue(any(list(line.coords) == [(-1, 0.5), (1, 0.5)] for line in lines))
        self.assertFalse(any(line.distance(LineString([(-0.8, 0), (0.8, 0)])) == 0 for line in lines))

    def test_colors_preserved_with_cross_protein_occlusion(self):
        rear = Panel(Polygon([(-2, -2), (2, -2), (2, 2), (-2, 2)]),
                     [LineString([(-2, 0), (2, 0)])], ink='black')
        front = Panel(Polygon([(-1, -1), (1, -1), (1, 1), (-1, 1)]),
                      [LineString([(-1, 0.5), (1, 0.5)])], ink='red')
        colors = visible_lines([rear, front], by_color=True)
        self.assertEqual(set(colors), {'black', 'red'})
        self.assertAlmostEqual(sum(line.length for line in colors['black']), 2)
        self.assertAlmostEqual(sum(line.length for line in colors['red']), 2)

    def test_panel_ink_metadata_read(self):
        svg = '<svg xmlns="http://www.w3.org/2000/svg"><g><polygon points="0,0 1,0 1,1 0,1" data-ink="red"/><path d="M0,0 L1,0"/></g></svg>'
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'input.svg'
            path.write_text(svg)
            self.assertEqual(read_panels(path)[0].ink, 'red')

    def test_panel_does_not_hide_its_own_lines(self):
        panel = Panel(Polygon([(0, 0), (1, 0), (1, 1), (0, 1)]),
                      [LineString([(0, 0.5), (1, 0.5)])])
        self.assertAlmostEqual(visible_lines([panel])[0].length, 1)

    def test_join_and_deduplicate_small_roundoff_gaps(self):
        lines = [LineString([(0, 0), (1, 0)]), LineString([(1.002, 0), (2, 0)]),
                 LineString([(1, 0), (0, 0)])]
        result = merge_lines(lines)
        self.assertEqual(len(result), 1)
        self.assertAlmostEqual(result[0].length, 2)

    def test_do_not_bridge_large_gaps(self):
        result = merge_lines([LineString([(0, 0), (1, 0)]), LineString([(1.1, 0), (2, 0)])])
        self.assertEqual(len(result), 2)

    def test_order_can_reverse_without_changing_length(self):
        source = [LineString([(3, 0), (2, 0)]), LineString([(1, 0), (0, 0)])]
        ordered = order_paths(source)
        self.assertEqual(list(ordered[0].coords), [(0, 0), (1, 0)])
        self.assertEqual(list(ordered[1].coords), [(2, 0), (3, 0)])
        self.assertEqual(sum(line.length for line in ordered), 2)

    def test_directed_joins_continue_smoothly_through_branch(self):
        source = [LineString([(0, 0), (1, 0)]), LineString([(1, -1), (1, 0)]),
                  LineString([(1, 0), (2, 0)]), LineString([(1, 0), (1, 1)])]
        self.assertEqual(len(merge_lines(source)), 4)
        joined = merge_directed_lines(source)
        self.assertEqual(len(joined), 2)
        self.assertIn([(0, 0), (1, 0), (2, 0)], [list(line.coords) for line in joined])
        self.assertIn([(1, -1), (1, 0), (1, 1)], [list(line.coords) for line in joined])
        self.assertTrue(shapely.union_all(source).equals(shapely.union_all(joined)))
        self.assertAlmostEqual(sum(line.length for line in joined), 4)

    def test_directed_join_keeps_gaps_and_first_duplicate_direction(self):
        source = [LineString([(0, 0), (1, 0)]), LineString([(1, 0), (0, 0)]),
                  LineString([(1.1, 0), (2, 0)])]
        joined = merge_directed_lines(source)
        self.assertEqual(len(joined), 2)
        self.assertIn([(0, 0), (1, 0)], [list(line.coords) for line in joined])
        self.assertAlmostEqual(sum(line.length for line in joined), 1.9)

    def test_directed_cycles_traverse_each_edge_once(self):
        source = [LineString([(0, 0), (1, 0)]), LineString([(1, 0), (1, 1)]),
                  LineString([(1, 1), (0, 0)])]
        joined = merge_directed_lines(source)
        self.assertEqual(len(joined), 1)
        self.assertTrue(joined[0].is_closed)
        self.assertAlmostEqual(joined[0].length, sum(line.length for line in source))

    def test_order_without_reversal_preserves_every_input_orientation(self):
        source = [LineString([(3, 0), (2, 0)]), LineString([(1, 0), (0, 0)])]
        ordered = order_paths(source, allow_reverse=False)
        self.assertEqual([list(line.coords) for line in ordered],
                         [[(1, 0), (0, 0)], [(3, 0), (2, 0)]])

    def test_clipping_preserves_backbone_direction_and_chain_groups(self):
        rear = Panel(Polygon([(-2, -2), (2, -2), (2, 2), (-2, 2)]),
                     [LineString([(2, 0), (-2, 0)])], chain_id='A')
        front = Panel(Polygon([(-1, -1), (1, -1), (1, 1), (-1, 1)]),
                      [LineString([(-1, 0.5), (1, 0.5)])], chain_id='B')
        groups = visible_lines([rear, front], by_chain=True, preserve_direction=True)
        self.assertEqual(set(groups), {('black', 'A'), ('black', 'B')})
        self.assertEqual(len(groups[('black', 'A')]), 2)
        for line in groups[('black', 'A')]:
            self.assertGreater(line.coords[0][0], line.coords[-1][0])
        self.assertAlmostEqual(sum(line.length for lines in groups.values() for line in lines), 4)

    def test_unsupported_paths_rejected(self):
        svg = '<svg xmlns="http://www.w3.org/2000/svg"><g><polygon points="0,0 1,0 1,1 0,1"/><path d="M0,0 C1,1 2,2 3,3"/></g></svg>'
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'input.svg'
            path.write_text(svg)
            with self.assertRaisesRegex(ValueError, 'simple M/L'):
                read_panels(path)

    def test_loop_fill_is_parallel_and_close(self):
        corners = ((0, 0), (4, 0), (4, 1), (0, 1))
        panel = Panel(Polygon(corners), loop_corners=corners, corners=corners)
        self.assertEqual(hatch_loops([panel], 1, 0.2), 4)
        for line in panel.lines:
            start, end = line.coords
            self.assertAlmostEqual(start[1], end[1])
            self.assertGreater(start[1], 0)
            self.assertLess(start[1], 1)

    def test_extra_border_paths_stay_inside_and_skip_loops(self):
        corners = ((0, 0), (4, 0), (4, 1), (0, 1))
        ribbon = Panel(Polygon(corners), corners=corners)
        loop = Panel(Polygon(corners), loop_corners=corners, corners=corners)
        self.assertEqual(reinforce_borders([ribbon, loop], 1, 3, 0.1), 4)
        self.assertEqual(len(loop.lines), 0)
        self.assertTrue(all(ribbon.surface.covers(line) for line in ribbon.lines))

    def test_transition_tracks_share_endpoints_without_crosswise_caps(self):
        a = ((0, -1), (1, -1), (1, 1), (0, 1))
        b = ((1, -1), (2, -0.2), (2, 0.2), (1, 1))
        panels = [Panel(Polygon(a), corners=a, loop_blend=(0, 0.5)),
                  Panel(Polygon(b), corners=b, loop_blend=(0.5, 1))]
        count, total = continuous_tracks(panels, 1, True, 0.15, 3, 0.1)
        self.assertEqual(count, 13)
        self.assertEqual(total, 26)
        for previous, following in zip(panels[0].lines, panels[1].lines):
            self.assertEqual(previous.coords[-1], following.coords[0])
            self.assertLess(previous.coords[0][0], previous.coords[-1][0])
            self.assertLess(following.coords[0][0], following.coords[-1][0])

    def test_rendered_transition_geometry_has_identical_seam_endpoints(self):
        for path in Path('output').glob('*-smooth.svg'):
            if 'plotter' in path.name:
                continue
            chains = {}
            for element in ET.parse(path).getroot().iter(SVG + 'polygon'):
                chain = element.attrib['data-chain']
                step = int(element.attrib['data-step'])
                corners = [tuple(map(float, point.split(',')))
                           for point in element.attrib['points'].split()]
                chains.setdefault(chain, {})[step] = (corners, element.attrib)
            for steps in chains.values():
                for i in range(len(steps) - 1):
                    previous, pa = steps[i]
                    following, fa = steps[i + 1]
                    self.assertEqual(previous[1], following[0])
                    self.assertEqual(previous[2], following[3])
                    self.assertEqual(pa['data-loop-b'], fa['data-loop-a'])

    def test_generated_exports_are_only_unfilled_paths(self):
        for path in Path('output').glob('*-plotter*.svg'):
            with self.subTest(path=path):
                root = ET.parse(path).getroot()
                self.assertEqual(root.attrib['width'], '210mm')
                self.assertEqual(root.attrib['height'], '297mm')
                self.assertTrue(root.findall('.//' + SVG + 'path'))
                for element in root.iter():
                    self.assertIn(element.tag.removeprefix(SVG), ('svg', 'title', 'desc', 'g', 'path'))
                    self.assertIn(element.attrib.get('fill', 'none'), ('none',))


if __name__ == '__main__':
    unittest.main()
