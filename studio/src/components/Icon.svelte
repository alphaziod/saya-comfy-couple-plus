<script lang="ts">
  // Small stroke icon set (Lucide-style paths) used by the shell and workbench toolbars.
  let { name, size = 16 }: { name: string; size?: number } = $props();
  const paths: Record<string, string> = {
    create: 'M12 3l2.2 5.2L20 9l-4.4 3.8L17 19l-5-3-5 3 1.4-6.2L4 9l5.8-.8z',
    casting: 'M3 3h7v9H3zM14 3h7v9h-7zM3 15h7v6H3zM14 15h7v6h-7z',
    gallery: 'M3 4h18v16H3zM3 16l5-5 4 4 3-3 6 6M15.5 7a1.5 1.5 0 100 3 1.5 1.5 0 000-3z',
    characters: 'M12 4a4 4 0 100 8 4 4 0 000-8zM4 21c1.5-4 4.5-6 8-6s6.5 2 8 6',
    models: 'M12 3l8 4.5v9L12 21l-8-4.5v-9zM12 12l8-4.5M12 12v9M12 12L4 7.5',
    studio: 'M6 3.5a2.5 2.5 0 100 5 2.5 2.5 0 000-5zM18 9.5a2.5 2.5 0 100 5 2.5 2.5 0 000-5zM6 15.5a2.5 2.5 0 100 5 2.5 2.5 0 000-5zM8.5 6H12a3 3 0 013 3v.5M8.5 18H12a3 3 0 003-3v-.5',
    data: 'M5 6c0-1.7 3.1-3 7-3s7 1.3 7 3-3.1 3-7 3-7-1.3-7-3zM5 6v12c0 1.7 3.1 3 7 3s7-1.3 7-3V6M5 12c0 1.7 3.1 3 7 3s7-1.3 7-3',
    settings: 'M12 9a3 3 0 100 6 3 3 0 000-6zM12 2v3M12 19v3M4.2 4.2l2.1 2.1M17.7 17.7l2.1 2.1M2 12h3M19 12h3M4.2 19.8l2.1-2.1M17.7 6.3l2.1-2.1',
    play: 'M6 4l14 8-14 8z',
    info: 'M12 3a9 9 0 100 18 9 9 0 000-18zM12 11v5M12 8h.01',
    compare: 'M12 3v18M5 7h4v10H5zM15 7h4v10h-4z',
    folder: 'M3 7h6l2 2h10v10H3z',
    copy: 'M8 8h12v12H8zM16 8V4H4v12h4',
    edit: 'M4 20l4-1 11-11-3-3L5 16z',
    redo: 'M4 12a8 8 0 0114-5l2 2M20 4v5h-5',
    trash: 'M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13',
    expand: 'M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5',
    more: 'M5 12h.01M12 12h.01M19 12h.01',
    queue: 'M4 6h16M4 12h16M4 18h10',
    save: 'M5 3h11l3 3v15H5zM8 3v5h7V3M8 21v-7h8v7',
    shuffle: 'M16 3h5v5M4 20L21 3M21 16v5h-5M15 15l6 6M4 4l5 5',
    close: 'M6 6l12 12M18 6L6 18',
    left: 'M15 6l-6 6 6 6',
    right: 'M9 6l6 6-6 6',
    star: 'M12 3l2.7 5.6 6.3.9-4.5 4.4 1 6.1L12 17l-5.5 3 1-6.1L3 9.5l6.3-.9z',
    check: 'M5 12l5 5 9-10',
    help: 'M12 3a9 9 0 100 18 9 9 0 000-18zM9.5 9a2.5 2.5 0 114 2c-1 .6-1.5 1.2-1.5 2.5M12 17h.01'
  };
</script>
<svg class="icon" viewBox="0 0 24 24" width={size} height={size} aria-hidden="true"><path d={paths[name] ?? ''}/></svg>
<style>.icon{stroke:currentColor;fill:none;stroke-width:1.6;stroke-linecap:round;stroke-linejoin:round;flex:none;display:block}</style>
