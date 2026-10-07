//! Map-owned v2 artifact admission into the Optimization internal travel model.
use super::{
    DenseTravelMatrix, InlineTravelModel, LocationId, OptimizationContractError, VehicleTypeId,
};
use std::collections::BTreeSet;
use veoveo_map_mcp::contract::{MapTravelModelUri, TravelModelArtifact, TravelModelMatrix};

pub fn decode_map_travel_model(
    bytes: &[u8],
    expected: Option<&MapTravelModelUri>,
) -> Result<InlineTravelModel, OptimizationContractError> {
    let artifact: TravelModelArtifact = serde_json::from_slice(bytes)
        .map_err(|_| invalid("Map travel-model artifact is not an admitted current document"))?;
    if expected.is_some_and(|uri| artifact.map_resource_uri.as_ref() != Some(uri)) {
        return Err(invalid(
            "Map travel-model artifact does not attest the selected resource",
        ));
    }
    let model = InlineTravelModel {
        location_ids: artifact
            .model
            .location_ids
            .into_iter()
            .map(|id| LocationId::parse(id.to_string()))
            .collect::<Result<_, _>>()?,
        cost_matrices: artifact
            .model
            .cost_matrices
            .into_iter()
            .map(matrix)
            .collect::<Result<_, _>>()?,
        transit_time_matrices: artifact
            .model
            .transit_time_matrices
            .into_iter()
            .map(matrix)
            .collect::<Result<_, _>>()?,
    };
    if model.location_ids.is_empty()
        || model.location_ids.iter().collect::<BTreeSet<_>>().len() != model.location_ids.len()
    {
        return Err(invalid(
            "Map travel-model locations must be nonempty and distinct",
        ));
    }
    let costs = model
        .cost_matrices
        .iter()
        .map(|matrix| &matrix.vehicle_type_id)
        .collect::<BTreeSet<_>>();
    let transits = model
        .transit_time_matrices
        .iter()
        .map(|matrix| &matrix.vehicle_type_id)
        .collect::<BTreeSet<_>>();
    if costs.is_empty()
        || costs.len() != model.cost_matrices.len()
        || (!transits.is_empty()
            && (transits != costs || transits.len() != model.transit_time_matrices.len()))
    {
        return Err(invalid(
            "Map travel-model matrices must identify distinct matching vehicle types",
        ));
    }
    for matrix in model
        .cost_matrices
        .iter()
        .chain(&model.transit_time_matrices)
    {
        matrix.validate("Map travel-model matrix")?;
        if matrix.dimension as usize != model.location_ids.len() {
            return Err(invalid(
                "Map travel-model dimension must match location order",
            ));
        }
    }
    Ok(model)
}

fn matrix(value: TravelModelMatrix) -> Result<DenseTravelMatrix, OptimizationContractError> {
    Ok(DenseTravelMatrix {
        vehicle_type_id: VehicleTypeId::parse(value.vehicle_type_id.to_string())?,
        dimension: value.dimension,
        values: value.values,
        unavailable_cells: value.unavailable_cells,
    })
}

fn invalid(message: &str) -> OptimizationContractError {
    OptimizationContractError::InvalidProblem(message.to_owned())
}
