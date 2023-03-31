# Version 0.9.0
* Bump `vulkano` version to 0.32
* Bump `imgui` version to 0.9
* Bump `winit` to version 0.27 for the examples
* Cache PersistentDescriptorSets instead of creating new each frame
* Add support for Gamma correction (necessary for nVidia wayland driver)

# Version 0.7.1

* Bump `vulkano` version to 0.23.0

# Version 0.7.0

* Bump `imgui-rs` version to 0.7.0
* Skip version 0.6 to match `imgui-rs`

# Version 0.5.0

* Bump `imgui-rs` version to 0.6.1
* Bump `vulkano` version to 0.20.0
* Bump `winit` version to 0.24.0
* Use `CpuBufferPool` from `vulkano` to allocate new vertex and index buffers each frame. I used to just create a new `CpuAccessibleBuffer` every frame, but this seems like a more correct solution.

# Version 0.4.0

* Bump `imgui-rs` version to 0.6.0

# Version 0.3.0

* Bump `imgui-rs` version to 0.5.0

# Version 0.2.0

* Create new vertex and index buffers in case the previous ones are still in use. Once used, the buffers should be cleaned up by vulkano.

# Version 0.1.0

Initial version.
