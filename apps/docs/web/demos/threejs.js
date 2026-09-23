// A procedural sculpture, rendered with ordinary Three.js materials.
// Rust owns the controls. This module owns the canvas and GPU resources.
export function onMount({ root, signal, inputs, onCleanup }) {
  // Async setup has its own rollback; the same cleanup also runs on unmount.
  const cleanups = [];
  let disposed = false;
  const own = (cleanup) => {
    if (disposed) {
      try { cleanup(); } catch (error) { console.error(error); }
    } else cleanups.push(cleanup);
  };
  function dispose() {
    if (disposed) return;
    disposed = true;
    for (const cleanup of cleanups.reverse()) {
      try { cleanup(); } catch (error) { console.error(error); }
    }
    cleanups.length = 0;
  }
  onCleanup(dispose);
  const report = (detail) => {
    if (!signal.aborted) root.dispatchEvent(new CustomEvent("gardenstatus", { detail }));
  };
  void Promise.all([
    import("three"),
    import("three/addons/environments/RoomEnvironment.js"),
  ]).then(([THREE, { RoomEnvironment }]) => {
    if (signal.aborted) return;
    const canvas = root.querySelector("canvas");
    const host = root.querySelector(".garden-scene");
    let frame = 0;
    const geometries = new Set();
    const materials = new Set();
    const textures = new Set();
    const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
    own(() => {
      cancelAnimationFrame(frame);
      for (const geometry of geometries) geometry.dispose();
      for (const material of materials) material.dispose();
      for (const texture of textures) texture.dispose();
      renderer.dispose();
      renderer.forceContextLoss();
    });
    // A single owner controls output conversion and all GPU resources.
    renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = 0.9;
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    const geometry = (value) => { geometries.add(value); return value; };
    const material = (value) => { materials.add(value); return value; };
    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(38, 1, 0.1, 60);
    let yaw = 0.22;
    let elevation = 0.22;
    let distance = 9.6;
    const sculpture = new THREE.Group();
    sculpture.position.set(0.35, 0, 0);
    sculpture.rotation.z = -0.18;
    scene.add(sculpture);
    const environment = new RoomEnvironment();
    let pmrem;
    try {
      pmrem = new THREE.PMREMGenerator(renderer);
      const target = pmrem.fromScene(environment, 0.04);
      own(() => target.dispose());
      scene.environment = target.texture;
    } finally {
      pmrem?.dispose();
      environment.dispose();
    }
    scene.add(new THREE.AmbientLight(0xa8c9ff, 0.3));
    const key = new THREE.DirectionalLight(0xd8e8ff, 3);
    key.position.set(3, 5, 4);
    scene.add(key);
    const rim = new THREE.PointLight(0x6dfbe1, 38, 15, 2);
    rim.position.set(-4, 1, 2);
    scene.add(rim);
    const warm = new THREE.PointLight(0xfc71b6, 30, 14, 2);
    warm.position.set(3, -1, -3);
    scene.add(warm);

    const palettes = {
      aurora: ["#36bdb4", "#278795", "#5968c9", "#ae79ca"],
      ember: ["#fda15a", "#d35b32", "#b63561", "#7955a8"],
      glacier: ["#c1efff", "#6acfeb", "#518bca", "#d1d9f1"],
    };
    const petals = [];
    const segments = 100;
    for (let index = 0; index < 24; index++) {
      const meshGeometry = geometry(new THREE.BufferGeometry());
      meshGeometry.setAttribute("position", new THREE.BufferAttribute(new Float32Array((segments + 1) * 15), 3));
      const indices = [];
      for (let step = 0; step < segments; step++) {
        for (let strip = 0; strip < 4; strip++) {
          const base = step * 5 + strip;
          indices.push(base, base + 1, base + 5, base + 1, base + 6, base + 5);
        }
      }
      meshGeometry.setIndex(indices);
      const meshMaterial = material(new THREE.MeshPhysicalMaterial({
        metalness: 0.88,
        roughness: 0.28,
        envMapIntensity: 0.72,
        clearcoat: 1,
        clearcoatRoughness: 0.18,
        iridescence: 0.38,
        iridescenceIOR: 1.35,
        side: THREE.DoubleSide,
      }));
      const petal = new THREE.Mesh(meshGeometry, meshMaterial);
      petal.userData.petal = index + 1;
      petals.push(petal);
      sculpture.add(petal);
    }
    function shape(value) {
      const bloom = value / 100;
      petals.forEach((petal, index) => {
        const positions = petal.geometry.attributes.position;
        const layer = index < 12 ? 1 : 0.73;
        const offset = index * Math.PI * 2 / 12 + (index < 12 ? 0 : 0.24);
        for (let step = 0; step <= segments; step++) {
          const t = step / segments;
          const arc = Math.PI * t;
          const radius = (0.13 + Math.sin(arc) * (1.1 + bloom * 1.12)) * layer;
          const angle = offset + arc * (0.45 + bloom * 0.6);
          const y = Math.cos(arc) * (2.0 - bloom * 0.18) * layer;
          const width = (0.015 + 0.16 * Math.sin(arc) ** 0.7) * layer;
          for (let edge = 0; edge < 5; edge++) {
            const side = edge / 2 - 1;
            const twist = arc * 1.5 + offset;
            const curvedRadius = radius + Math.cos(side * Math.PI / 2) * width * 0.48;
            positions.setXYZ(step * 5 + edge,
              Math.cos(angle) * curvedRadius - Math.sin(angle) * width * side,
              y + Math.sin(twist) * width * side * 0.8,
              Math.sin(angle) * curvedRadius + Math.cos(angle) * width * side);
          }
        }
        positions.needsUpdate = true;
        petal.geometry.computeVertexNormals();
        petal.geometry.computeBoundingSphere();
      });
    }
    const coreMaterial = material(new THREE.MeshPhysicalMaterial({
      color: 0xf0eaff, emissive: 0x969ce0, emissiveIntensity: 0.3,
      metalness: 0.28, roughness: 0.19, clearcoat: 1,
    }));
    sculpture.add(new THREE.Mesh(geometry(new THREE.SphereGeometry(0.52, 48, 32)), coreMaterial));
    const glowCanvas = document.createElement("canvas");
    glowCanvas.width = glowCanvas.height = 128;
    const ctx = glowCanvas.getContext("2d");
    const gradient = ctx.createRadialGradient(64, 64, 0, 64, 64, 64);
    gradient.addColorStop(0, "#b8baff60");
    gradient.addColorStop(0.2, "#7199d130");
    gradient.addColorStop(1, "#6695ff00");
    ctx.fillStyle = gradient;
    ctx.fillRect(0, 0, 128, 128);
    const glowTexture = new THREE.CanvasTexture(glowCanvas);
    textures.add(glowTexture);
    const glow = new THREE.Sprite(material(new THREE.SpriteMaterial({
      map: glowTexture, transparent: true, blending: THREE.AdditiveBlending, depthWrite: false,
    })));
    glow.scale.setScalar(3.5);
    sculpture.add(glow);

    // Orbital traces ground the sculpture without a post-processing pipeline.
    const orbitMaterial = material(new THREE.MeshBasicMaterial({ color: 0x6480a3, transparent: true, opacity: 0.25 }));
    for (const radius of [2.35, 2.8, 3.25]) {
      const ring = new THREE.Mesh(geometry(new THREE.TorusGeometry(radius, 0.004, 4, 180)), orbitMaterial);
      ring.rotation.x = Math.PI / 2;
      ring.position.y = -2.3;
      scene.add(ring);
    }
    const satelliteGeometry = geometry(new THREE.SphereGeometry(0.028, 10, 8));
    const satellites = new THREE.InstancedMesh(satelliteGeometry, material(new THREE.MeshBasicMaterial({ color: 0xb7dfed })), 48);
    const transform = new THREE.Object3D();
    for (let index = 0; index < 48; index++) {
      const angle = index * 2.399963;
      const radius = 2.6 + 0.5 * Math.sin(index * 1.71);
      transform.position.set(Math.cos(angle) * radius, Math.sin(index * 2.16) * 1.8, Math.sin(angle) * radius);
      transform.scale.setScalar(index % 7 === 0 ? 1.8 : 0.7);
      transform.updateMatrix();
      satellites.setMatrixAt(index, transform.matrix);
    }
    sculpture.add(satellites);
    own(() => satellites.dispose());
    const starPositions = [];
    for (let index = 0; index < 170; index++) {
      // Deterministic placement makes the fixed initial view reproducible.
      starPositions.push(Math.sin(index * 127.1) * 11, Math.sin(index * 311.7) * 6, -5 - (index % 13) * 0.35);
    }
    const stars = geometry(new THREE.BufferGeometry());
    stars.setAttribute("position", new THREE.Float32BufferAttribute(starPositions, 3));
    scene.add(new THREE.Points(stars, material(new THREE.PointsMaterial({ color: 0x96bad2, size: 0.018, transparent: true, opacity: 0.5 }))));

    let selected = inputs.selected.get();
    let palette = inputs.palette.get();
    function colorize() {
      const colors = palettes[palette] || palettes.aurora;
      petals.forEach((petal, index) => {
        const color = new THREE.Color(colors[index % colors.length]);
        petal.material.color.copy(color);
        petal.material.emissive.copy(color);
        petal.material.emissiveIntensity = selected === index + 1 ? 0.65 : 0.035;
        petal.material.roughness = selected === index + 1 ? 0.18 : 0.28;
      });
      rim.color.set(colors[0]);
      warm.color.set(colors[2]);
    }
    function updateCamera() {
      camera.position.set(Math.sin(yaw) * Math.cos(elevation) * distance, Math.sin(elevation) * distance, Math.cos(yaw) * Math.cos(elevation) * distance);
      camera.lookAt(0.2, -0.05, 0);
    }
    function render() {
      if (signal.aborted || disposed) return;
      updateCamera();
      renderer.render(scene, camera);
    }
    function resize() {
      const { width, height } = host.getBoundingClientRect();
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      distance = width < 520 ? 11.6 : 9.6;
      sculpture.position.y = width < 520 ? -0.85 : 0;
      camera.updateProjectionMatrix();
      render();
    }
    const observer = new ResizeObserver(resize);
    observer.observe(host);
    own(() => observer.disconnect());
    own(inputs.bloom.subscribe((value) => { shape(value); render(); }));
    own(inputs.palette.subscribe((value) => { palette = value; colorize(); render(); }));
    own(inputs.selected.subscribe((value) => { selected = value; colorize(); render(); }));

    const motionPreference = matchMedia("(prefers-reduced-motion: reduce)");
    let paused = inputs.paused.get();
    let previous = 0;
    function animate(now) {
      frame = 0;
      if (signal.aborted || paused || motionPreference.matches || document.hidden) return;
      const delta = previous ? Math.min((now - previous) / 1000, 0.05) : 0;
      previous = now;
      sculpture.rotation.y += delta * 0.13;
      satellites.rotation.y -= delta * 0.05;
      render();
      frame = requestAnimationFrame(animate);
    }
    function schedule() {
      cancelAnimationFrame(frame);
      frame = 0;
      previous = 0;
      if (!paused && !motionPreference.matches && !document.hidden && !signal.aborted) frame = requestAnimationFrame(animate);
      render();
    }
    own(inputs.paused.subscribe((value) => { paused = value; schedule(); }));
    function listen(target, type, listener) {
      target.addEventListener(type, listener, { signal });
      own(() => target.removeEventListener(type, listener));
    }
    listen(motionPreference, "change", schedule);
    listen(document, "visibilitychange", schedule);

    const pointer = new THREE.Vector2();
    const raycaster = new THREE.Raycaster();
    function pick(index) {
      if (!signal.aborted) root.dispatchEvent(new CustomEvent("gardenpick", { detail: index }));
    }
    let drag = null;
    listen(canvas, "pointerdown", (event) => {
      drag = { x: event.clientX, y: event.clientY, yaw, elevation, moved: false };
      canvas.setPointerCapture(event.pointerId);
    });
    listen(canvas, "pointermove", (event) => {
      if (!drag) return;
      const dx = event.clientX - drag.x;
      const dy = event.clientY - drag.y;
      if (Math.abs(dx) + Math.abs(dy) > 5) drag.moved = true;
      if (drag.moved) {
        yaw = drag.yaw - dx * 0.007;
        elevation = Math.max(-0.6, Math.min(0.8, drag.elevation + dy * 0.005));
        render();
      }
    });
    listen(canvas, "pointerup", (event) => {
      if (drag && !drag.moved) {
        const rect = canvas.getBoundingClientRect();
        pointer.set((event.clientX - rect.left) / rect.width * 2 - 1, -(event.clientY - rect.top) / rect.height * 2 + 1);
        raycaster.setFromCamera(pointer, camera);
        const hit = raycaster.intersectObjects(petals)[0];
        if (hit) pick(hit.object.userData.petal);
      }
      drag = null;
    });
    listen(canvas, "pointercancel", () => { drag = null; });
    listen(canvas, "keydown", (event) => {
      if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
        event.preventDefault();
        pick(event.key === "ArrowRight" ? selected % 24 + 1 : (selected + 22) % 24 + 1);
      }
    });
    resize();
    report(motionPreference.matches ? "Three.js ready · reduced motion" : "Three.js ready · a living Rust connection");
  }).catch((error) => {
    dispose();
    report(`Garden unavailable: ${error.message}. This scene needs WebGL 2.`);
    if (!signal.aborted) console.error(error);
  });
}
