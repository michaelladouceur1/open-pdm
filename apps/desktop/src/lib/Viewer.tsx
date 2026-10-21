import { useEffect, useRef } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import "./Viewer.css";

type Theme = "light" | "dark";

interface ViewerProps {
  // Flat vertex positions [x,y,z,...] and triangle indices [i,j,k,...].
  positions?: number[];
  indices?: number[];
  theme?: Theme;
}

// Scene colours per theme. The light background matches the surrounding
// light UI; the dark background is a lighter grey than a pure near-black so
// the model and grid stay legible.
const VIEWER_COLORS: Record<Theme, { background: number; gridMajor: number; gridMinor: number }> = {
  light: { background: 0xdddddd, gridMajor: 0xb8bfc9, gridMinor: 0xd2d7de },
  dark: { background: 0x222222, gridMajor: 0x565d6e, gridMinor: 0x424856 },
};

function makeGrid(theme: Theme): THREE.GridHelper {
  const { gridMajor, gridMinor } = VIEWER_COLORS[theme];
  const grid = new THREE.GridHelper(10, 10, gridMajor, gridMinor);
  grid.rotation.x = Math.PI / 2; // Lay the grid flat in the Z-up plane.
  return grid;
}

export default function Viewer({ positions = [], indices = [], theme = "light" }: ViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  // The three.js objects deliberately stay in refs: React state would treat
  // them as immutable snapshots and three relies on object identity.
  const sceneRef = useRef<THREE.Scene | null>(null);
  const cameraRef = useRef<THREE.PerspectiveCamera | null>(null);
  const controlsRef = useRef<OrbitControls | null>(null);
  const solidRef = useRef<THREE.Mesh | null>(null);
  const gridRef = useRef<THREE.GridHelper | null>(null);
  const themeRef = useRef<Theme>(theme);
  themeRef.current = theme;

  // Set up the scene once, on mount.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const scene = new THREE.Scene();
    scene.background = new THREE.Color(VIEWER_COLORS[themeRef.current].background);

    const camera = new THREE.PerspectiveCamera(45, 1, 0.1, 1000);
    camera.up.set(0, 0, 1); // CAD convention: Z is up.

    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;

    scene.add(new THREE.AmbientLight(0xffffff, 0.75));
    const key = new THREE.DirectionalLight(0xffffff, 1.6);
    key.position.set(4, -6, 8);
    scene.add(key);

    const grid = makeGrid(themeRef.current);
    gridRef.current = grid;
    scene.add(grid);

    // Keep the drawing buffer and camera in sync with the container's CSS box.
    // The canvas's CSS box is pinned to the container (stylesheet + the inline
    // style set here), so the element always fills it; this only has to keep
    // the render resolution and camera aspect correct.
    let lastW = 0;
    let lastH = 0;
    const resize = () => {
      const w = Math.max(container.clientWidth, 1);
      const h = Math.max(container.clientHeight, 1);
      if (w === lastW && h === lastH) return;
      lastW = w;
      lastH = h;
      // `updateStyle = false`: size the buffer only, never three's inline px.
      renderer.setSize(w, h, false);
      renderer.domElement.style.width = "100%";
      renderer.domElement.style.height = "100%";
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(container);
    // Re-measure over the first few frames: in dev the component CSS is
    // injected asynchronously, so the container can still be settling when we
    // mount (which is why the first frame could otherwise stay half-width).
    let settleRaf = 0;
    let settleFrames = 0;
    const settle = () => {
      resize();
      if (settleFrames++ < 10) settleRaf = requestAnimationFrame(settle);
    };
    settleRaf = requestAnimationFrame(settle);

    let raf = 0;
    const animate = () => {
      raf = requestAnimationFrame(animate);
      controls.update();
      renderer.render(scene, camera);
    };
    animate();

    sceneRef.current = scene;
    cameraRef.current = camera;
    controlsRef.current = controls;

    return () => {
      cancelAnimationFrame(raf);
      cancelAnimationFrame(settleRaf);
      observer.disconnect();
      controls.dispose();
      renderer.dispose();
      renderer.domElement.remove();
      if (gridRef.current) {
        gridRef.current.geometry.dispose();
        (gridRef.current.material as THREE.Material).dispose();
        gridRef.current = null;
      }
      sceneRef.current = null;
      cameraRef.current = null;
      controlsRef.current = null;
    };
  }, []);

  // Repaint the scene when the app theme changes: swap the background and
  // rebuild the grid with colours suited to the new background.
  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene) return;

    scene.background = new THREE.Color(VIEWER_COLORS[theme].background);

    if (gridRef.current) {
      scene.remove(gridRef.current);
      gridRef.current.geometry.dispose();
      (gridRef.current.material as THREE.Material).dispose();
      const grid = makeGrid(theme);
      gridRef.current = grid;
      scene.add(grid);
    }
  }, [theme]);

  // Rebuild the mesh whenever the geometry changes.
  useEffect(() => {
    const scene = sceneRef.current;
    const camera = cameraRef.current;
    const controls = controlsRef.current;

    const removeSolid = () => {
      if (!solidRef.current) return;
      scene?.remove(solidRef.current);
      solidRef.current.geometry.dispose();
      (solidRef.current.material as THREE.Material).dispose();
      solidRef.current = null;
    };

    if (!scene || !camera || !controls) return;

    removeSolid();
    if (!indices || indices.length === 0) return;

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
    geometry.setIndex(indices);
    geometry.computeVertexNormals();
    geometry.computeBoundingSphere();

    const solid = new THREE.Mesh(
      geometry,
      new THREE.MeshStandardMaterial({ color: 0x7fb2ff, metalness: 0.15, roughness: 0.55 }),
    );
    solidRef.current = solid;
    scene.add(solid);

    const sphere = geometry.boundingSphere!;
    const radius = sphere.radius || 1;
    controls.target.copy(sphere.center);
    camera.position.set(radius * 1.7, -radius * 1.7, radius * 1.7).add(sphere.center);
    camera.near = radius / 100;
    camera.far = radius * 100;
    camera.updateProjectionMatrix();
    controls.update();

    return removeSolid;
  }, [positions, indices]);

  return <div className="viewer" ref={containerRef} />;
}
