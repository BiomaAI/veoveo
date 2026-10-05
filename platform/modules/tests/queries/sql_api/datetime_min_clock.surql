DEFINE FUNCTION fn::kernel::own::at_v1($at: datetime) -> object { RETURN { at: array::min([$at, time::now()]) }; } PERMISSIONS FULL;
