#define_import_path bevy_pbr::skinning

#import bevy_pbr::mesh_types::SkinnedMesh

#ifdef SKINNED

#ifdef MESH_BINDGROUP_1
    @group(1) @binding(1) var<uniform> joint_matrices: SkinnedMesh;
    #ifdef MOTION_VECTOR_PREPASS
    @group(1) @binding(4) var<uniform> previous_joint_matrices: SkinnedMesh;
    #endif // MOTION_VECTOR_PREPASS
#else 
    @group(2) @binding(1) var<uniform> joint_matrices: SkinnedMesh;
    #ifdef MOTION_VECTOR_PREPASS
    @group(2) @binding(4) var<uniform> previous_joint_matrices: SkinnedMesh;
    #endif // MOTION_VECTOR_PREPASS
#endif


fn skin_model(
    indexes: vec4<u32>,
    weights: vec4<f32>,
) -> mat4x4<f32> {
    return weights.x * joint_matrices.data[indexes.x]
        + weights.y * joint_matrices.data[indexes.y]
        + weights.z * joint_matrices.data[indexes.z]
        + weights.w * joint_matrices.data[indexes.w];
}

fn inverse_transpose_3x3m(in: mat3x3<f32>) -> mat3x3<f32> {
    let x = cross(in[1], in[2]);
    let y = cross(in[2], in[0]);
    let z = cross(in[0], in[1]);
    let det = dot(in[2], z);
    return mat3x3<f32>(
        x / det,
        y / det,
        z / det
    );
}

fn skin_normals(
    model: mat4x4<f32>,
    normal: vec3<f32>,
) -> vec3<f32> {
    return normalize(
        inverse_transpose_3x3m(
            mat3x3<f32>(
                model[0].xyz,
                model[1].xyz,
                model[2].xyz
            )
        ) * normal
    );
}

#ifdef MOTION_VECTOR_PREPASS
fn previous_skin_model(
    indexes: vec4<u32>,
    weights: vec4<f32>,
) -> mat4x4<f32> {
    return weights.x * previous_joint_matrices.data[indexes.x]
        + weights.y * previous_joint_matrices.data[indexes.y]
        + weights.z * previous_joint_matrices.data[indexes.z]
        + weights.w * previous_joint_matrices.data[indexes.w];
}
#endif // MOTION_VECTOR_PREPASS
#endif
