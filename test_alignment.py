"""Regression tests for rigid domain alignment and shared-frame rendering."""
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import numpy as np
from scipy.spatial.transform import Rotation

import render
from prepare_domain3 import rigid_fit
from plotter import build_parser
from select_complexes import representatives
from make_diverse_nine import diverse_indices
from scipy.spatial.distance import pdist, squareform


class AlignmentTests(unittest.TestCase):
    def test_rigid_fit_recovers_rotation_and_preserves_binder_pose(self):
        rng = np.random.default_rng(17)
        target = rng.normal(size=(172, 3))
        binder = rng.normal(size=(100, 3)) + [4, 2, 1]
        rotation = Rotation.from_euler('xyz', [23, -48, 75], degrees=True).as_matrix()
        translation = np.array([17, -3, 9])
        mobile = target @ rotation + translation
        mobile_binder = binder @ rotation + translation
        fitted_rotation, fitted_translation, rmsd = rigid_fit(mobile, target)
        self.assertLess(rmsd, 1e-12)
        self.assertAlmostEqual(np.linalg.det(fitted_rotation), 1)
        np.testing.assert_allclose(mobile_binder @ fitted_rotation + fitted_translation,
                                   binder, atol=1e-12)

    def test_visual_selection_keeps_one_representative_per_similar_pair(self):
        points = np.array([[0., 0.], [0.01, 0.], [5., 5.], [5.01, 5.]])
        distances = squareform(pdist(points))
        selected = representatives(distances, 2)
        self.assertEqual(len(selected), 2)
        self.assertEqual(selected, sorted(selected))
        self.assertEqual(sum(index < 2 for index in selected), 1)
        self.assertEqual(representatives(distances, 4), [0, 1, 2, 3])

    def test_maximin_diversity_avoids_near_duplicate_views(self):
        points = np.array([[0.], [0.01], [5.], [5.01], [10.], [10.01]])
        distances = squareform(pdist(points))
        chosen = diverse_indices(distances, 3)
        self.assertEqual(len(set(chosen)), 3)
        self.assertEqual({i // 2 for i in chosen}, {0, 1, 2})
        self.assertEqual(chosen, diverse_indices(distances, 3))

    def test_fit_never_reflects_a_structure(self):
        target = np.random.default_rng(3).normal(size=(172, 3))
        reflected = target * [-1, 1, 1]
        rotation, _, rmsd = rigid_fit(reflected, target)
        self.assertAlmostEqual(np.linalg.det(rotation), 1)
        self.assertGreater(rmsd, 0.1)

    def test_fixed_camera_avoids_pca_and_passes_shared_frame_to_plotter(self):
        segment = [(np.array([np.sin(i), i, np.cos(i)]),
                    np.array([1., 0., 0.]), 'C', 'A') for i in range(6)]
        with tempfile.TemporaryDirectory() as directory:
            args = build_parser().parse_args(['input.svg', '-o', str(Path(directory) / 'out.svg')])
            args.input = Path('synthetic.pdb')
            args.samples = 4
            args.rotate = [0, 0, 0]
            args.size = 200
            args.lines = 3
            args.title = 'Alignment test'
            args.source = None
            args.illustration = False
            args.png = None
            args.fixed_camera = True
            args.view_bounds = [-10, -10, 10, 10]
            args.chain_colors = {}
            with patch.object(render, 'camera', side_effect=AssertionError('PCA must not run')):
                with patch.object(render.plotter, 'convert') as convert:
                    render.render([segment], args, {'C': 6})
                    first = convert.call_args.args[0].frame_bounds
                    shifted = [(p + [2, 1, 0], side, kind, chain) for p, side, kind, chain in segment]
                    render.render([shifted], args, {'C': 6})
                    second = convert.call_args.args[0].frame_bounds
            np.testing.assert_allclose(first, second)


if __name__ == '__main__':
    unittest.main()
