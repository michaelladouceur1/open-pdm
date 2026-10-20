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

    const resize = () => {
      const w = container.clientWidth;
      const h = Math.max(container.clientHeight, 1);
      // Let three.js set the canvas CSS size as well as its drawing buffer;
      // with updateStyle=false the HiDPI buffer makes the canvas lay out at
      // devicePixelRatio× the container and get clipped.
      renderer.setSize(w, h);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(container);

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
    width: 100%;
    height: 100%;
    min-height: 320px;
    background: #181a20;
    border-radius: 8px;
    overflow: hidden;
  }
  .viewer :global(canvas) {
    display: block;
  }
</style>
