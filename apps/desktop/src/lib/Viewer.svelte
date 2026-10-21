<script lang="ts">
  import { onMount } from "svelte";
  import * as THREE from "three";
  import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

  // Flat vertex positions [x,y,z,...] and triangle indices [i,j,k,...].
  let { positions = [] as number[], indices = [] as number[] } = $props();

  let container: HTMLDivElement;
  // `ready` is $state so the mesh $effect re-runs once the scene exists.
  // The three.js objects deliberately stay plain variables: $state would
  // deep-proxy them and break three's internal identity checks.
  let ready = $state(false);

  let renderer: THREE.WebGLRenderer;
  let scene: THREE.Scene;
  let camera: THREE.PerspectiveCamera;
  let controls: OrbitControls;
  let solid: THREE.Mesh | null = null;
  let raf = 0;

  onMount(() => {
    scene = new THREE.Scene();
    scene.background = new THREE.Color(0x181a20);

    camera = new THREE.PerspectiveCamera(45, 1, 0.1, 1000);
    camera.up.set(0, 0, 1); // CAD convention: Z is up.

    renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);

    controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;

    scene.add(new THREE.AmbientLight(0xffffff, 0.75));
    const key = new THREE.DirectionalLight(0xffffff, 1.6);
    key.position.set(4, -6, 8);
    scene.add(key);

    const grid = new THREE.GridHelper(10, 10, 0x3a3f4b, 0x2a2e37);
    grid.rotation.x = Math.PI / 2; // Lay the grid flat in the Z-up plane.
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

    const animate = () => {
      raf = requestAnimationFrame(animate);
      controls.update();
      renderer.render(scene, camera);
    };
    animate();
    ready = true;

    return () => {
      ready = false;
      cancelAnimationFrame(raf);
      cancelAnimationFrame(settleRaf);
      observer.disconnect();
      controls.dispose();
      renderer.dispose();
      renderer.domElement.remove();
    };
  });

  $effect(() => {
    const pos = positions;
    const idx = indices;
    if (!ready) return;

    if (solid) {
      scene.remove(solid);
      solid.geometry.dispose();
      (solid.material as THREE.Material).dispose();
      solid = null;
    }
    if (!idx || idx.length === 0) return;

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
    geometry.setIndex(idx);
    geometry.computeVertexNormals();
    geometry.computeBoundingSphere();

    solid = new THREE.Mesh(
      geometry,
      new THREE.MeshStandardMaterial({ color: 0x7fb2ff, metalness: 0.15, roughness: 0.55 }),
    );
    scene.add(solid);

    const sphere = geometry.boundingSphere!;
    const radius = sphere.radius || 1;
    controls.target.copy(sphere.center);
    camera.position.set(radius * 1.7, -radius * 1.7, radius * 1.7).add(sphere.center);
    camera.near = radius / 100;
    camera.far = radius * 100;
    camera.updateProjectionMatrix();
    controls.update();
  });
</script>

<div class="viewer" bind:this={container}></div>

<style>
  .viewer {
    /* Pin to the (positioned) viewport box so it always fills it, independent
       of flex/percentage-height resolution quirks across engines. */
    position: absolute;
    inset: 0;
    background: #181a20;
    overflow: hidden;
  }
  /* Pin the canvas to the container instead of relying on three.js's inline
     style sizing, so it always fills the box at any devicePixelRatio. */
  .viewer :global(canvas) {
    display: block;
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
</style>
